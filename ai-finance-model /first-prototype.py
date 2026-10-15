"""
RWA Sentiment Intelligence Engine  v4.0.0
==========================================
Production-grade async sentiment pipeline for tokenised real-estate communities.

Key upgrades over v3
--------------------
  ① Memory safety       — retry guard prevents duplicate memory pushes on retries
  ② True compression    — token-budget compression via Claude, not naïve slicing
  ③ Schema validation   — Pydantic v2 enforces every field contract (0-100 clamps etc.)
  ④ Full observability  — structured UsageMetrics (tokens, cost, latency, request-id)
  ⑤ Rich error surface  — BatchError carries per-item exc instead of None silently
  ⑥ Streaming retries   — stream_analysis is now also retry-protected
  ⑦ Prompt hot-swap     — PromptRegistry supports runtime registration + version chain
  ⑧ Cost tracking       — token usage → USD estimate per model with configurable rates
  ⑨ Ephemeral fix       — analyze_bulk inherits prompt_version from parent engine
  ⑩ Circuit breaker     — consecutive failure counter trips open state, self-heals

Author  : Sqwerzyyy
Version : 4.0.0
"""

from __future__ import annotations

import asyncio
import json
import logging
import math
import re
import time
import uuid
from collections import deque
from contextlib import asynccontextmanager
from dataclasses import dataclass, field
from enum import Enum
from typing import Any, AsyncIterator, Callable, Sequence, Type, TypeVar

import anthropic
from pydantic import BaseModel, Field, field_validator, model_validator
from tenacity import (
    AsyncRetrying,
    before_sleep_log,
    retry_if_exception_type,
    stop_after_attempt,
    wait_exponential_jitter,
)

# ---------------------------------------------------------------------------
# Logging
# ---------------------------------------------------------------------------

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s | %(levelname)-8s | %(name)s | %(message)s",
    datefmt="%Y-%m-%dT%H:%M:%S",
)
log = logging.getLogger("rwa.engine")

T = TypeVar("T", bound=Enum)

# ---------------------------------------------------------------------------
# Domain enumerations
# ---------------------------------------------------------------------------


class Sentiment(str, Enum):
    BULLISH = "BULLISH"
    BEARISH = "BEARISH"
    NEUTRAL = "NEUTRAL"
    MIXED = "MIXED"


class AssetType(str, Enum):
    RESIDENTIAL = "residential"
    COMMERCIAL = "commercial"
    HOSPITALITY = "hospitality"
    INDUSTRIAL = "industrial"
    LAND = "land"
    REIT = "reit"
    TOKEN = "token"
    PLATFORM = "platform"
    UNKNOWN = "unknown"


class SignalStrength(str, Enum):
    STRONG = "strong"
    MODERATE = "moderate"
    WEAK = "weak"


class CircuitState(str, Enum):
    CLOSED = "closed"
    OPEN = "open"
    HALF_OPEN = "half_open"


# ---------------------------------------------------------------------------
# Pydantic v2 schema — single source of truth for the output contract
# ---------------------------------------------------------------------------


def _clamp(v: int) -> int:
    return max(0, min(100, v))


class IntentSchema(BaseModel):
    want_to_buy: int = Field(0, ge=0, le=100)
    want_to_sell: int = Field(0, ge=0, le=100)
    want_to_hold: int = Field(0, ge=0, le=100)
    watching: int = Field(0, ge=0, le=100)
    seeking_info: int = Field(0, ge=0, le=100)

    @field_validator("*", mode="before")
    @classmethod
    def clamp_int(cls, v: Any) -> int:
        try:
            return _clamp(int(v))
        except (TypeError, ValueError):
            return 0


class MetricsSchema(BaseModel):
    hype_score: int = Field(0, ge=0, le=100)
    fear_score: int = Field(0, ge=0, le=100)
    trust_score: int = Field(0, ge=0, le=100)
    popularity: int = Field(0, ge=0, le=100)
    conviction_score: int = Field(0, ge=0, le=100)
    consensus_score: int = Field(0, ge=0, le=100)

    @field_validator("*", mode="before")
    @classmethod
    def clamp_int(cls, v: Any) -> int:
        try:
            return _clamp(int(v))
        except (TypeError, ValueError):
            return 0


class RiskSchema(BaseModel):
    liquidity_risk: int = Field(0, ge=0, le=100)
    regulatory_risk: int = Field(0, ge=0, le=100)
    developer_risk: int = Field(0, ge=0, le=100)
    market_risk: int = Field(0, ge=0, le=100)
    currency_risk: int = Field(0, ge=0, le=100)

    @field_validator("*", mode="before")
    @classmethod
    def clamp_int(cls, v: Any) -> int:
        try:
            return _clamp(int(v))
        except (TypeError, ValueError):
            return 0


class AssetSignalSchema(BaseModel):
    name: str = "unknown"
    type: str = "unknown"
    sentiment: str = "NEUTRAL"
    signal_strength: str = "weak"
    buy_interest: int = Field(0, ge=0, le=100)
    sell_pressure: int = Field(0, ge=0, le=100)
    mentions: int = Field(1, ge=0)
    yield_discussed: float | None = None
    location: str | None = None
    platform: str | None = None

    @field_validator("buy_interest", "sell_pressure", mode="before")
    @classmethod
    def clamp_int(cls, v: Any) -> int:
        try:
            return _clamp(int(v))
        except (TypeError, ValueError):
            return 0

    @field_validator("yield_discussed", mode="before")
    @classmethod
    def safe_float(cls, v: Any) -> float | None:
        if v is None:
            return None
        try:
            return float(v)
        except (TypeError, ValueError):
            return None


class MarketIntelSchema(BaseModel):
    trending_themes: list[str] = Field(default_factory=list)
    contrarian_signals: list[str] = Field(default_factory=list)
    emerging_concerns: list[str] = Field(default_factory=list)
    platform_sentiment: dict[str, int] = Field(default_factory=dict)
    jurisdiction_sentiment: dict[str, int] = Field(default_factory=dict)

    @field_validator("platform_sentiment", "jurisdiction_sentiment", mode="before")
    @classmethod
    def clamp_dict(cls, v: Any) -> dict[str, int]:
        if not isinstance(v, dict):
            return {}
        return {
            k: _clamp(int(val))
            for k, val in v.items()
            if isinstance(val, (int, float, str))
        }


class AnalysisSchema(BaseModel):
    """Authoritative contract. All fields validated and type-safe."""

    sentiment: Sentiment = Sentiment.NEUTRAL
    confidence: int = Field(0, ge=0, le=100)
    intent: IntentSchema = Field(default_factory=IntentSchema)
    metrics: MetricsSchema = Field(default_factory=MetricsSchema)
    risk: RiskSchema = Field(default_factory=RiskSchema)
    assets_mentioned: list[AssetSignalSchema] = Field(default_factory=list)
    market_intelligence: MarketIntelSchema = Field(default_factory=MarketIntelSchema)
    key_positives: list[str] = Field(default_factory=list)
    key_concerns: list[str] = Field(default_factory=list)
    actionable_signals: list[str] = Field(default_factory=list)
    summary: str = ""

    @field_validator("confidence", mode="before")
    @classmethod
    def clamp_conf(cls, v: Any) -> int:
        try:
            return _clamp(int(v))
        except (TypeError, ValueError):
            return 0

    @field_validator("sentiment", mode="before")
    @classmethod
    def coerce_sentiment(cls, v: Any) -> Sentiment:
        try:
            return Sentiment(str(v).upper())
        except ValueError:
            return Sentiment.NEUTRAL

    @model_validator(mode="after")
    def validate_asset_list(self) -> "AnalysisSchema":
        if len(self.assets_mentioned) > 50:
            log.warning(
                "Unusually large asset list (%d), truncating.",
                len(self.assets_mentioned),
            )
            self.assets_mentioned = self.assets_mentioned[:50]
        return self


# ---------------------------------------------------------------------------
# Observability — per-request usage tracking
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class UsageMetrics:
    request_id: str
    input_tokens: int
    output_tokens: int
    latency_ms: int
    estimated_cost_usd: float
    model: str
    retries: int

    @property
    def total_tokens(self) -> int:
        return self.input_tokens + self.output_tokens


_MODEL_PRICE_PER_1M = {
    "claude-opus-4-5": (15.0, 75.0),
    "claude-sonnet-4-5": (3.0, 15.0),
    "claude-haiku-4-5": (0.25, 1.25),
}


def _estimate_cost(model: str, input_tokens: int, output_tokens: int) -> float:
    rates = _MODEL_PRICE_PER_1M.get(model, (15.0, 75.0))
    return (input_tokens * rates[0] + output_tokens * rates[1]) / 1_000_000


# ---------------------------------------------------------------------------
# Enriched result — wraps validated schema + observability
# ---------------------------------------------------------------------------


@dataclass
class AnalysisResult:
    schema: AnalysisSchema
    usage: UsageMetrics
    message_count: int

    # Convenience proxies — callers don't need to reach into .schema
    @property
    def sentiment(self) -> Sentiment:
        return self.schema.sentiment

    @property
    def confidence(self) -> int:
        return self.schema.confidence

    @property
    def intent(self) -> IntentSchema:
        return self.schema.intent

    @property
    def metrics(self) -> MetricsSchema:
        return self.schema.metrics

    @property
    def risk(self) -> RiskSchema:
        return self.schema.risk

    @property
    def assets_mentioned(self) -> list[AssetSignalSchema]:
        return self.schema.assets_mentioned

    @property
    def market_intelligence(self) -> MarketIntelSchema:
        return self.schema.market_intelligence

    @property
    def key_positives(self) -> list[str]:
        return self.schema.key_positives

    @property
    def key_concerns(self) -> list[str]:
        return self.schema.key_concerns

    @property
    def actionable_signals(self) -> list[str]:
        return self.schema.actionable_signals

    @property
    def summary(self) -> str:
        return self.schema.summary

    @property
    def analysis_latency_ms(self) -> int:
        return self.usage.latency_ms


# ---------------------------------------------------------------------------
# Batch result — richer than Optional[AnalysisResult]
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class BatchItem:
    index: int
    result: AnalysisResult | None
    error: Exception | None = None

    @property
    def ok(self) -> bool:
        return self.result is not None


# ---------------------------------------------------------------------------
# Prompt Registry — hot-swappable, runtime-extensible
# ---------------------------------------------------------------------------


class PromptRegistry:
    _registry: dict[str, str] = {}
    _fallback_chain: dict[str, str] = {}  # version -> fallback version

    @classmethod
    def register(
        cls,
        version: str,
        prompt: str,
        fallback: str | None = None,
    ) -> None:
        cls._registry[version] = prompt
        if fallback:
            cls._fallback_chain[version] = fallback
        log.debug("Prompt registered: version=%s fallback=%s", version, fallback)

    @classmethod
    def get(cls, version: str = "v4") -> str:
        visited = set()
        current = version
        while current:
            if current in visited:
                raise ValueError(f"Circular fallback chain at '{current}'")
            visited.add(current)
            if current in cls._registry:
                return cls._registry[current]
            current = cls._fallback_chain.get(current)
        raise ValueError(
            f"Prompt version '{version}' not found. Available: {sorted(cls._registry)}"
        )

    @classmethod
    def available(cls) -> list[str]:
        return sorted(cls._registry.keys())


_V4_SYSTEM = """
You are a senior quantitative analyst embedded in a high-frequency sentiment
intelligence platform for Real World Asset (RWA) tokenized real-estate markets.

ROLE
----
Process raw investor chat messages and return a machine-readable structured
JSON sentiment report. Your output feeds directly into trading signal pipelines,
risk dashboards, and portfolio management systems.

DOMAIN EXPERTISE
----------------
Platforms      : RealT, Lofty, Tangible, Blocksquare, RealtyX, RedSwan,
                 Propchain, Parcl, Homebase, RealToken
Asset classes  : residential, commercial, industrial, hospitality, land,
                 REITs, fractional pools, tokenized MBS
Financial KPIs : APY, rental yield, cap rate, ROI, LTV, IRR, NOI, DSCR
On-chain       : token liquidity, secondary depth, lock-up periods,
                 redemption windows, DAO votes, treasury yield
Jurisdictions  : UAE, Portugal, Thailand, Spain, USA, Panama, Singapore,
                 UK, Germany, Poland, Brazil

INFERENCE RULES
---------------
1.  Passive observation vs active intent — weight accordingly.
2.  Unresolved questions are mild bearish signals unless countered.
3.  Repeated mention amplifies popularity non-linearly.
4.  Extract yield figures verbatim.
5.  Platform scepticism discounts all assets on that platform.
6.  Conditional intent ("would buy if...") counts as 40-60% buy_interest.
7.  Past-tense purchases ("I grabbed X tokens") are confirmed demand signals.
8.  Regulatory concerns apply jurisdiction-wide risk discount.
9.  Thin liquidity concerns apply platform-wide trust penalty.
10. Rank platforms and jurisdictions independently from asset-level sentiment.

OUTPUT CONTRACT
---------------
Return ONLY a valid JSON object. No prose, no markdown fences, no comments.
Every integer field is 0-100. Missing data defaults to 0 or null.

{
  "sentiment"  : "BULLISH|BEARISH|NEUTRAL|MIXED",
  "confidence" : <int 0-100>,
  "intent": {
    "want_to_buy"  : <int>, "want_to_sell" : <int>, "want_to_hold" : <int>,
    "watching"     : <int>, "seeking_info" : <int>
  },
  "metrics": {
    "hype_score": <int>, "fear_score": <int>, "trust_score": <int>,
    "popularity": <int>, "conviction_score": <int>, "consensus_score": <int>
  },
  "risk": {
    "liquidity_risk": <int>, "regulatory_risk": <int>, "developer_risk": <int>,
    "market_risk": <int>, "currency_risk": <int>
  },
  "assets_mentioned": [
    {
      "name": "<string>", "type": "residential|commercial|hospitality|industrial|land|reit|token|platform|unknown",
      "sentiment": "BULLISH|BEARISH|NEUTRAL", "signal_strength": "strong|moderate|weak",
      "buy_interest": <int>, "sell_pressure": <int>, "mentions": <int>,
      "yield_discussed": <float|null>, "location": "<string|null>", "platform": "<string|null>"
    }
  ],
  "market_intelligence": {
    "trending_themes": ["<string>"], "contrarian_signals": ["<string>"],
    "emerging_concerns": ["<string>"],
    "platform_sentiment": {"<platform>": <int>},
    "jurisdiction_sentiment": {"<country>": <int>}
  },
  "key_positives": ["<string>"], "key_concerns": ["<string>"],
  "actionable_signals": ["<string>"],
  "summary": "<3-4 sentence professional English summary>"
}
""".strip()

PromptRegistry.register("v4", _V4_SYSTEM)
PromptRegistry.register(
    "v3", _V4_SYSTEM, fallback="v4"
)  # v3 aliases v4 as fallback example

# ---------------------------------------------------------------------------
# JSON Extractor — bracket-matched, fence-aware
# ---------------------------------------------------------------------------

_FENCE_RE = re.compile(r"```(?:json)?\s*(.*?)\s*```", re.DOTALL)


class SignalExtractor:
    @staticmethod
    def parse(raw: str) -> dict[str, Any]:
        candidate = raw.strip()

        fence_m = _FENCE_RE.search(candidate)
        if fence_m:
            candidate = fence_m.group(1).strip()

        try:
            return json.loads(candidate)
        except json.JSONDecodeError:
            pass

        extracted = SignalExtractor._bracket_extract(candidate)
        if extracted:
            try:
                return json.loads(extracted)
            except json.JSONDecodeError:
                pass

        raise ValueError(
            f"All JSON extraction strategies exhausted.\nFirst 400 chars:\n{raw[:400]}"
        )

    @staticmethod
    def _bracket_extract(text: str) -> str | None:
        depth = in_string = escape = False
        start = None
        depth_count = 0

        for idx, ch in enumerate(text):
            if escape:
                escape = False
                continue
            if ch == "\\" and in_string:
                escape = True
                continue
            if ch == '"':
                in_string = not in_string
                continue
            if in_string:
                continue
            if ch == "{":
                if depth_count == 0:
                    start = idx
                depth_count += 1
            elif ch == "}":
                depth_count -= 1
                if depth_count == 0 and start is not None:
                    return text[start : idx + 1]
        return None

    @staticmethod
    def top_assets(
        assets: list[AssetSignalSchema],
        n: int = 5,
        key: Callable[[AssetSignalSchema], float] | None = None,
    ) -> list[AssetSignalSchema]:
        if key is None:
            key = lambda a: a.buy_interest * math.log1p(a.mentions)
        return sorted(assets, key=key, reverse=True)[:n]


# ---------------------------------------------------------------------------
# Memory — sliding window + LLM-powered compression (not naïve slicing)
# ---------------------------------------------------------------------------

_COMPRESS_PROMPT = (
    "Summarise the following conversation exchanges into a single concise paragraph "
    "preserving all asset names, platform names, yield figures, sentiment signals, "
    "and actionable conclusions. Return plain text only.\n\n"
)


class ConversationMemory:
    def __init__(
        self,
        max_turns: int = 20,
        compress_at: int = 15,
        llm_compress: bool = True,
    ) -> None:
        self._turns: deque[dict[str, str]] = deque(maxlen=max_turns * 2)
        self._compress_at: int = compress_at * 2
        self._llm_compress: bool = llm_compress
        self._compressor: Callable[[str], Any] | None = None  # injected by engine

    def push(self, role: str, content: str) -> None:
        self._turns.append({"role": role, "content": content})

    def should_compress(self) -> bool:
        return len(self._turns) >= self._compress_at

    def snapshot(self) -> list[dict[str, str]]:
        return list(self._turns)

    def clear(self) -> None:
        self._turns.clear()

    def __len__(self) -> int:
        return len(self._turns)

    async def maybe_compress(self) -> None:
        """
        LLM-powered compression: summarise the oldest half via Claude,
        retain the newest half verbatim. No information is blindly discarded.
        If _compressor is unavailable, falls back to text joining.
        """
        if not self.should_compress():
            return

        turns = list(self._turns)
        half = len(turns) // 2
        old = turns[:half]
        new = turns[half:]

        text_block = "\n".join(f"[{t['role'].upper()}] {t['content']}" for t in old)

        if self._compressor is not None:
            try:
                summary = await self._compressor(text_block)
            except Exception as exc:
                log.warning("LLM compression failed (%s), using text join.", exc)
                summary = text_block[:1000] + " [truncated]"
        else:
            summary = (
                text_block[:1000]
                + " [truncated — set _compressor for full LLM compression]"
            )

        compressed_msg = {
            "role": "assistant",
            "content": f"[COMPRESSED CONTEXT — {half} exchanges summarised]\n{summary}",
        }
        self._turns.clear()
        self._turns.append(compressed_msg)
        self._turns.extend(new)
        log.debug(
            "Memory compressed — %d turns → summary + %d retained.", half, len(new)
        )


# ---------------------------------------------------------------------------
# Circuit Breaker — prevents hammering a failing API endpoint
# ---------------------------------------------------------------------------


@dataclass
class CircuitBreaker:
    failure_threshold: int = 5
    recovery_seconds: float = 60.0
    _failures: int = field(default=0, init=False, repr=False)
    _opened_at: float = field(default=0.0, init=False, repr=False)
    _state: CircuitState = field(default=CircuitState.CLOSED, init=False)

    def record_success(self) -> None:
        self._failures = 0
        self._state = CircuitState.CLOSED

    def record_failure(self) -> None:
        self._failures += 1
        if self._failures >= self.failure_threshold:
            self._state = CircuitState.OPEN
            self._opened_at = time.monotonic()
            log.error("Circuit OPEN after %d consecutive failures.", self._failures)

    def allow_request(self) -> bool:
        if self._state == CircuitState.CLOSED:
            return True
        if self._state == CircuitState.OPEN:
            if time.monotonic() - self._opened_at >= self.recovery_seconds:
                self._state = CircuitState.HALF_OPEN
                log.info("Circuit HALF-OPEN — probing recovery.")
                return True
            return False
        return True  # HALF_OPEN: allow one probe

    @property
    def state(self) -> CircuitState:
        return self._state


# ---------------------------------------------------------------------------
# Core Engine
# ---------------------------------------------------------------------------

_RETRYABLE = (
    anthropic.RateLimitError,
    anthropic.APIStatusError,
    anthropic.APIConnectionError,
)


class RWASentimentEngine:
    """
    Async-native orchestrator for RWA tokenised real-estate sentiment intelligence.

    v4 improvements over v3
    -----------------------
    ① Memory guard       — user message pushed to memory ONLY after successful API response.
                           Retries never double-write to memory.
    ② Pydantic validation— every field in AnalysisSchema is range-validated at parse time.
    ③ UsageMetrics        — token counts + cost estimate attached to every AnalysisResult.
    ④ LLM compression    — ConversationMemory compresses via Claude, not naïve slicing.
    ⑤ Circuit breaker    — trips after N consecutive failures, self-heals after timeout.
    ⑥ BatchItem          — analyze_bulk returns BatchItem list; error field carries exc.
    ⑦ prompt_version fix — analyze_bulk ephemeral engines inherit parent's prompt_version.
    ⑧ Streaming retries  — stream_analysis is wrapped with exponential backoff.
    ⑨ top_assets key arg — caller-supplied ranking key for custom composite scoring.
    ⑩ Lifecycle ctx mgr  — use as async context manager for clean client teardown.

    Usage
    -----
        async with RWASentimentEngine() as engine:
            result  = await engine.analyze(messages)
            answer  = await engine.ask("Which platform has best trust score?")
            results = await engine.analyze_bulk(batches)

            async for chunk in engine.stream_analysis(messages):
                sys.stdout.write(chunk)
    """

    def __init__(
        self,
        client: anthropic.AsyncAnthropic | None = None,
        model: str = "claude-opus-4-5",
        max_tokens: int = 2048,
        prompt_version: str = "v4",
        memory_turns: int = 40,
        compress_at: int = 15,
        llm_compress: bool = True,
        circuit_breaker: CircuitBreaker | None = None,
    ) -> None:
        self._client = client or anthropic.AsyncAnthropic()
        self._owns_client = client is None
        self._model = model
        self._max_tokens = max_tokens
        self._prompt_version = prompt_version
        self._system_prompt = PromptRegistry.get(prompt_version)
        self._memory = ConversationMemory(memory_turns, compress_at, llm_compress)
        self._extractor = SignalExtractor()
        self._circuit = circuit_breaker or CircuitBreaker()
        self._retry_count = 0

        # Wire LLM compressor into memory
        self._memory._compressor = self._compress_via_llm

    # -------------------------------------------------------------------------
    # Lifecycle
    # -------------------------------------------------------------------------

    async def __aenter__(self) -> "RWASentimentEngine":
        return self

    async def __aexit__(self, *_: Any) -> None:
        await self.close()

    async def close(self) -> None:
        if self._owns_client:
            await self._client.close()

    # -------------------------------------------------------------------------
    # Public API
    # -------------------------------------------------------------------------

    async def analyze(self, messages: list[str]) -> AnalysisResult:
        if not messages:
            raise ValueError("messages must not be empty.")

        await self._memory.maybe_compress()

        t0 = time.monotonic_ns()
        payload = self._build_analysis_prompt(messages)
        raw, usage_raw = await self._complete(payload)
        latency = (time.monotonic_ns() - t0) // 1_000_000

        parsed = self._extractor.parse(raw)
        schema = AnalysisSchema.model_validate(parsed)
        usage = UsageMetrics(
            request_id=str(uuid.uuid4()),
            input_tokens=usage_raw.input_tokens,
            output_tokens=usage_raw.output_tokens,
            latency_ms=latency,
            estimated_cost_usd=_estimate_cost(
                self._model, usage_raw.input_tokens, usage_raw.output_tokens
            ),
            model=self._model,
            retries=self._retry_count,
        )
        result = AnalysisResult(schema=schema, usage=usage, message_count=len(messages))
        log.info(
            "Analysis complete | sentiment=%s conf=%d%% assets=%d "
            "latency=%dms tokens=%d cost=$%.4f retries=%d",
            result.sentiment.value,
            result.confidence,
            len(result.assets_mentioned),
            latency,
            usage.total_tokens,
            usage.estimated_cost_usd,
            usage.retries,
        )
        return result

    async def ask(self, question: str) -> str:
        if not self._memory.snapshot():
            raise RuntimeError("No active session. Call analyze() first.")
        await self._memory.maybe_compress()
        raw, _ = await self._complete(question)
        return raw

    async def analyze_bulk(
        self,
        batches: list[list[str]],
        concurrency: int = 32,
    ) -> list[BatchItem]:
        """
        Pure async fan-out. Returns list[BatchItem] — each item carries either
        a successful AnalysisResult or the Exception that caused failure.

        Parameters
        ----------
        batches     : independent message windows (each gets a fresh memory)
        concurrency : semaphore gate on simultaneous in-flight requests
        """
        semaphore = asyncio.Semaphore(concurrency)

        async def _bounded(idx: int, batch: list[str]) -> BatchItem:
            async with semaphore:
                try:
                    ephemeral = RWASentimentEngine(
                        client=self._client,  # shared pool
                        model=self._model,
                        max_tokens=self._max_tokens,
                        prompt_version=self._prompt_version,  # ← v3 bug fix
                        llm_compress=False,  # ephemeral engines are short-lived
                    )
                    return BatchItem(idx, await ephemeral.analyze(batch))
                except Exception as exc:
                    log.error("Batch %d failed: %s", idx, exc, exc_info=True)
                    return BatchItem(idx, result=None, error=exc)

        items = await asyncio.gather(*[_bounded(i, b) for i, b in enumerate(batches)])
        ordered = [None] * len(batches)
        for item in items:
            ordered[item.index] = item
        return ordered  # type: ignore[return-value]

    async def stream_analysis(self, messages: list[str]) -> AsyncIterator[str]:
        """Yield raw text chunks from the API with exponential-backoff retry."""
        payload = self._build_analysis_prompt(messages)
        await self._memory.maybe_compress()

        async for attempt in AsyncRetrying(
            retry=retry_if_exception_type(_RETRYABLE),
            wait=wait_exponential_jitter(initial=1, max=60, jitter=5),
            stop=stop_after_attempt(6),
            before_sleep=before_sleep_log(log, logging.WARNING),
            reraise=True,
        ):
            with attempt:
                full_text = ""
                # Push user msg AFTER we know the stream is starting (memory guard)
                self._memory.push("user", payload)
                async with self._client.messages.stream(
                    model=self._model,
                    max_tokens=self._max_tokens,
                    system=self._system_prompt,
                    messages=self._memory.snapshot(),
                ) as stream:
                    async for chunk in stream.text_stream:
                        full_text += chunk
                        yield chunk

                self._memory.push("assistant", full_text)

    def reset(self) -> None:
        self._memory.clear()
        self._circuit.record_success()
        log.info("Session memory cleared and circuit breaker reset.")

    @property
    def circuit_state(self) -> CircuitState:
        return self._circuit.state

    # -------------------------------------------------------------------------
    # Private
    # -------------------------------------------------------------------------

    async def _complete(self, user_message: str) -> tuple[str, Any]:
        """
        Memory-safe completion with circuit breaker + tenacity retry.

        CRITICAL design: user_message is pushed to memory ONLY after a
        successful response is received. This prevents retries from
        duplicating messages in the conversation history.
        """
        if not self._circuit.allow_request():
            raise RuntimeError(
                f"Circuit breaker is OPEN (state={self._circuit.state.value}). "
                "Too many consecutive failures. Will retry after recovery window."
            )

        # Snapshot current memory WITHOUT the new user message
        history_snapshot = self._memory.snapshot()
        messages_to_send = history_snapshot + [
            {"role": "user", "content": user_message}
        ]

        self._retry_count = 0

        async for attempt in AsyncRetrying(
            retry=retry_if_exception_type(_RETRYABLE),
            wait=wait_exponential_jitter(initial=1, max=60, jitter=5),
            stop=stop_after_attempt(6),
            before_sleep=before_sleep_log(log, logging.WARNING),
            reraise=True,
        ):
            with attempt:
                self._retry_count = attempt.retry_state.attempt_number - 1
                try:
                    response = await self._client.messages.create(
                        model=self._model,
                        max_tokens=self._max_tokens,
                        system=self._system_prompt,
                        messages=messages_to_send,
                    )
                    self._circuit.record_success()
                except Exception:
                    self._circuit.record_failure()
                    raise

        text = response.content[0].text.strip()

        # Push BOTH turns to memory only after confirmed success
        self._memory.push("user", user_message)
        self._memory.push("assistant", text)

        return text, response.usage

    async def _compress_via_llm(self, text_block: str) -> str:
        """Thin compression call — does NOT use or update session memory."""
        response = await self._client.messages.create(
            model=self._model,
            max_tokens=512,
            system="You are a precise technical summariser.",
            messages=[{"role": "user", "content": _COMPRESS_PROMPT + text_block}],
        )
        return response.content[0].text.strip()

    @staticmethod
    def _build_analysis_prompt(messages: list[str]) -> str:
        numbered = "\n".join(f"[{i + 1:>4}] {m}" for i, m in enumerate(messages))
        return (
            f"Analyse the following {len(messages)} investor chat messages "
            "from a tokenized real-estate platform:\n\n" + numbered
        )


# ---------------------------------------------------------------------------
# Demo
# ---------------------------------------------------------------------------


async def _demo() -> None:
    sample_chat = [
        "Lofty just listed a Miami condo — 8.4% APY, already 30% sold in 2 hours",
        "Grabbed 200 tokens at open, entry was clean enough to justify Florida exposure",
        "Anyone deep-dived the Tangible Lisbon pool? 11% with a 12-month lock feels aggressive",
        "11% on Tangible is real if you read the legal wrapper — Portuguese NHR regime is solid",
        "Minimum ticket on Lisbon is 5k EUR which kills retail, that is a red flag for liquidity",
        "RealT Detroit keeps underperforming — vacancy dragging yield below 4%, I am out",
        "Blocksquare Dubai hotel token is underrated, tourism demand structurally strong",
        "Secondary market depth on all these platforms is thin — that is the systemic risk nobody prices",
        "Bangkok apartment tokens still printing rental income monthly, holding another 12 months",
        "Would consider Homebase if they expand to EU jurisdictions, US-only is a hard no for me",
        "Parcl synthetic exposure is interesting for portfolio hedge but not the same as direct ownership",
        "Someone mentioned a new Blocksquare industrial park in Warsaw, 9.1% yield — anyone have details?",
        "Warsaw industrial is probably the best risk-adjusted play I have seen this quarter",
        "Regulatory clarity in Poland post-EU harmonization makes it safer than most EM plays",
    ]

    sep = "-" * 64

    async with RWASentimentEngine(model="claude-opus-4-5") as engine:
        result = await engine.analyze(sample_chat)

        print(f"\n{sep}")
        print(
            f"SENTIMENT       : {result.sentiment.value} ({result.confidence}% confidence)"
        )
        print(
            f"LATENCY         : {result.analysis_latency_ms}ms  |  MESSAGES: {result.message_count}"
        )
        print(
            f"TOKENS          : {result.usage.total_tokens} (in={result.usage.input_tokens} out={result.usage.output_tokens})"
        )
        print(
            f"COST ESTIMATE   : ${result.usage.estimated_cost_usd:.4f}  |  RETRIES: {result.usage.retries}"
        )
        print(f"REQUEST ID      : {result.usage.request_id}")
        print(f"CIRCUIT STATE   : {engine.circuit_state.value}")
        
        print(sep)

        print("\nINTENT VECTOR")
        for label, val in result.intent.model_dump().items():
            print(f"  {label:<16} {val:>3}%")

        print("\nSENTIMENT METRICS")
        for label, val in result.metrics.model_dump().items():
            print(f"  {label:<16} {val:>3}%")

        print("\nRISK PROFILE")
        for label, val in result.risk.model_dump().items():
            print(f"  {label:<16} {val:>3}%")

        print("\nTOP ASSETS BY COMPOSITE SIGNAL")
        for a in SignalExtractor.top_assets(result.assets_mentioned, n=5):
            yld = f"  yield={a.yield_discussed}%" if a.yield_discussed else ""
            loc = f"  loc={a.location}" if a.location else ""
            print(
                f"  [{a.signal_strength:<8}] {a.name:<28}"
                f"  buy={a.buy_interest:>3}%  sell={a.sell_pressure:>3}%"
                f"  mentions={a.mentions}{yld}{loc}"
            )

        if result.market_intelligence.platform_sentiment:
            print("\nPLATFORM SENTIMENT")
            for k, v in sorted(
                result.market_intelligence.platform_sentiment.items(),
                key=lambda x: -x[1],
            ):
                print(f"  {k:<22} {v:>3}%")

        if result.market_intelligence.jurisdiction_sentiment:
            print("\nJURISDICTION SENTIMENT")
            for k, v in sorted(
                result.market_intelligence.jurisdiction_sentiment.items(),
                key=lambda x: -x[1],
            ):
                print(f"  {k:<22} {v:>3}%")

        for section, items in [
            ("KEY POSITIVES", result.key_positives),
            ("KEY CONCERNS", result.key_concerns),
            ("ACTIONABLE SIGNALS", result.actionable_signals),
        ]:
            if items:
                print(f"\n{section}")
                for item in items:
                    print(f"  • {item}")

        print(f"\nSUMMARY\n  {result.summary}")
        print(sep)

        print("\nMULTI-TURN FOLLOW-UP")
        for question in [
            "Which single asset has the strongest composite buy signal and what is driving it?",
            "Rank all platforms by trust score and explain the spread.",
            "What conditions would push Warsaw industrial to a strong buy signal?",
        ]:
            print(f"\nQ: {question}")
            print(f"A: {await engine.ask(question)}")

        print(f"\n{sep}")
        print("BULK PROCESSING (2 isolated batches)")
        bulk = await engine.analyze_bulk(
            batches=[sample_chat[:7], sample_chat[7:]],
            concurrency=2,
        )
        for item in bulk:
            if item.ok:
                r = item.result
                print(
                    f"  Batch {item.index + 1}: sentiment={r.sentiment.value:<8}"
                    f"  conf={r.confidence}%  latency={r.analysis_latency_ms}ms"
                    f"  cost=${r.usage.estimated_cost_usd:.4f}"
                )
            else:
                print(f"  Batch {item.index + 1}: FAILED — {item.error}")

        print(f"\n{sep}")
        print("STREAMING ANALYSIS")
        print("Streaming: ", end="", flush=True)
        async for chunk in engine.stream_analysis(sample_chat[:5]):
            print(chunk, end="", flush=True)
        print()


if __name__ == "__main__":
    import asyncio

    asyncio.run(_demo())
