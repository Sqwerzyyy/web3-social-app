from dataclasses import dataclass, field
from datetime import datetime
from decimal import Decimal, ROUND_HALF_UP


@dataclass
class RWAPosition:
    token_address: str
    total_supply: int
    tokens_held: int
    purchase_price_usd: Decimal
    current_price_usd: Decimal
    rental_income_usd: Decimal
    purchase_date: datetime
    currency: str = "USD"

    def __post_init__(self):
        if not self.token_address or not self.token_address.strip():
            raise ValueError("token_address cannot be empty")
        if self.total_supply <= 0:
            raise ValueError("total_supply must be positive")
        if self.tokens_held <= 0:
            raise ValueError("tokens_held must be positive")
        if self.tokens_held > self.total_supply:
            raise ValueError("tokens_held cannot exceed total_supply")
        if self.purchase_price_usd <= 0:
            raise ValueError("purchase_price_usd must be positive")
        if self.current_price_usd <= 0:
            raise ValueError("current_price_usd must be positive")
        if self.rental_income_usd < 0:
            raise ValueError("rental_income_usd cannot be negative")
        if self.purchase_date > datetime.now():
            raise ValueError("purchase_date cannot be in the future")

        self.purchase_price_usd = Decimal(str(self.purchase_price_usd))
        self.current_price_usd = Decimal(str(self.current_price_usd))
        self.rental_income_usd = Decimal(str(self.rental_income_usd))

    def _round(self, value: Decimal) -> Decimal:
        return value.quantize(Decimal("0.01"), rounding=ROUND_HALF_UP)
    
    @property
    def ownership_pct(self) -> Decimal:
        return self._round(Decimal(self.tokens_held) / Decimal(self.total_supply) * 100)

    @property
    def cost_basis(self) -> Decimal:
        return self._round(Decimal(self.tokens_held) * self.purchase_price_usd)

    @property
    def market_value(self) -> Decimal:
        return self._round(Decimal(self.tokens_held) * self.current_price_usd)

    @property
    def unrealized_pnl(self) -> Decimal:
        return self._round(self.market_value - self.cost_basis)

    @property
    def total_pnl(self) -> Decimal:
        return self._round(self.unrealized_pnl + self.rental_income_usd)

    @property
    def roi_pct(self) -> Decimal:
        return self._round((self.total_pnl / self.cost_basis) * 100)

    def holding_days(self, as_of: datetime | None = None) -> int:
        reference = as_of or datetime.now()
        return (reference - self.purchase_date).days

    def annualized_roi(self, as_of: datetime | None = None) -> Decimal:
        days = self.holding_days(as_of)
        if days == 0:
            return Decimal("0.00")
        return self._round((self.roi_pct / Decimal(days)) * 365)

    def to_dict(self, as_of: datetime | None = None) -> dict:
        return {
            "token_address": self.token_address,
            "currency": self.currency,
            "ownership_pct": self.ownership_pct,
            "cost_basis": self.cost_basis,
            "market_value": self.market_value,
            "unrealized_pnl": self.unrealized_pnl,
            "rental_income": self.rental_income_usd,
            "total_pnl": self.total_pnl,
            "roi_pct": self.roi_pct,
            "annualized_roi": self.annualized_roi(as_of),
            "holding_days": self.holding_days(as_of),
        }

    def __str__(self) -> str:
        d = self.to_dict()
        return (
            f"Asset:            {d['token_address']}\n"
            f"Currency:         {d['currency']}\n"
            f"Ownership:        {d['ownership_pct']}%\n"
            f"Cost basis:       {d['cost_basis']:,}\n"
            f"Market value:     {d['market_value']:,}\n"
            f"Unrealized PnL:   {d['unrealized_pnl']:,}\n"
            f"Rental income:    {d['rental_income']:,}\n"
            f"Total PnL:        {d['total_pnl']:,}\n"
            f"ROI:              {d['roi_pct']}%\n"
            f"Annualized ROI:   {d['annualized_roi']}%\n"
            f"Holding period:   {d['holding_days']} days"
        )


position = RWAPosition(
    token_address="0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", #why we have here this adress???
    total_supply=1000,
    tokens_held=50,
    purchase_price_usd=Decimal("400.00"),
    current_price_usd=Decimal("460.00"),
    rental_income_usd=Decimal("1200.00"),
    purchase_date=datetime(2024, 2, 2),
)

print(position)