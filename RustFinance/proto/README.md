# Protobuf Contracts

Rust codegen runs from `build.rs`.

Go codegen:

```sh
protoc -I proto \
  --go_out=. --go_opt=paths=source_relative \
  --go-grpc_out=. --go-grpc_opt=paths=source_relative \
  proto/rwa/financial/v1/financial_engine.proto
```
