<div align="center">
  <img src="assets/logo.jpg" alt="Logo" width="20%">
</div>

# x402-facilitator

> The first x402 facilitator implementation for Monad blockchain, built with Rust, with additional support for Solana blockhain.

## About x402

The [x402 protocol](https://docs.cdp.coinbase.com/x402/docs/overview) is a proposed standard for making blockchain payments directly through HTTP using native `402 Payment Required` status code.

Servers declare payment requirements for specific routes. Clients send cryptographically signed payment payloads. Facilitators verify and settle payments on-chain.

## Getting Started

### Run facilitator

```shell
docker build -t x402-facilitator .
docker run --env-file .env -p 8080:8080 x402-facilitator
```

See the [Facilitator](#facilitator) section below for full usage details

## Facilitator

The `x402-facilitator` crate (this repo) provides a runnable x402 facilitator binary. The _Facilitator_ role simplifies adoption of x402 by handling:
- **Payment verification**: Confirming that client-submitted payment payloads match the declared requirements.
- **Payment settlement**: Submitting validated payments to the blockchain and monitoring their confirmation.

By using a Facilitator, servers (sellers) do not need to:
- Connect directly to a blockchain.
- Implement complex cryptographic or blockchain-specific payment logic.

Instead, they can rely on the Facilitator to perform verification and settlement, reducing operational overhead and accelerating x402 adoption.
The Facilitator **never holds user funds**. It acts solely as a stateless verification and execution layer for signed payment payloads.

For a detailed overview of the x402 payment flow and Facilitator role, see the [x402 protocol documentation](https://docs.cdp.coinbase.com/x402/docs/overview).

### Usage

#### 1. Provide environment variables

Create a `.env` file or set environment variables directly. Example `.env`:

```dotenv
HOST=0.0.0.0
PORT=8080
RPC_URL_MONAD_TESTNET=https://testnet-rpc.monad.xyz
RPC_URL_MONAD=https://rpc.monad.xyz
SIGNER_TYPE=private-key
EVM_PRIVATE_KEY=0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
SOLANA_PRIVATE_KEY=FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF
RUST_LOG=info
```

**Important:**
The supported networks are determined by which RPC URLs you provide:
- If you set only `RPC_URL_MONAD_TESTNET`, then only Monad Testnet network is supported.
- If you set both `RPC_URL_MONAD_TESTNET` and `RPC_URL_MONAD`, then both Monad Testnet and Monad Mainnet are supported.
- If an RPC URL for a network is missing, that network will not be available for settlement or verification.

#### 2. Build and Run with Docker

```shell
docker build -t x402-facilitator .
docker run --env-file .env -p 8080:8080 x402-facilitator
```

The container:
* Exposes port `8080` (or a port you configure with `PORT` environment variable).
* Starts on http://localhost:8080 by default.
* Requires minimal runtime dependencies (based on `debian:bullseye-slim`).

#### 3. Point your application to your Facilitator

If you are building an x402-powered application, update the Facilitator URL to point to your self-hosted instance.

> ℹ️ **Tip:** For production deployments, ensure your Facilitator is reachable via HTTPS and protect it against public abuse.

<details>
<summary>If you use Hono and x402-hono</summary>
From [x402.org Quickstart for Sellers](https://x402.gitbook.io/x402/getting-started/quickstart-for-sellers):

```typescript
import { Hono } from "hono";
import { serve } from "@hono/node-server";
import { paymentMiddleware } from "x402-hono";

const app = new Hono();

// Configure the payment middleware
app.use(paymentMiddleware(
  "0xYourAddress", // Your receiving wallet address
  {
    "/protected-route": {
      price: "$0.10",
      network: "monad-testnet",
      config: {
        description: "Access to premium content",
      }
    }
  },
  {
    url: "http://your-validator.url/", // 👈 Your self-hosted Facilitator
  }
));

// Implement your protected route
app.get("/protected-route", (c) => {
  return c.json({ message: "This content is behind a paywall" });
});

serve({
  fetch: app.fetch,
  port: 3000
});
```

</details>

### Configuration

The service reads configuration via `.env` file or directly through environment variables.

Available variables:

* `RUST_LOG`: Logging level (e.g., `info`, `debug`, `trace`),
* `HOST`: HTTP host to bind to (default: `0.0.0.0`),
* `PORT`: HTTP server port (default: `8080`),
* `SIGNER_TYPE` (required): Type of signer to use. Only `private-key` is supported now,
* `EVM_PRIVATE_KEY` (required): Private key in hex for EVM networks, like `0xdeadbeef...`,
* `SOLANA_PRIVATE_KEY` (required): Private key in hex for Solana networks, like `0xdeadbeef...`,
* `RPC_URL_MONAD`: RPC endpoint for Monad mainnet.
* `RPC_URL_MONAD_TESTNET`: RPC endpoint for Monad testnet.
* `RPC_URL_SOLANA`: RPC endpoint for Solana mainnet.
* `RPC_URL_SOLANA_DEVNET`: RPC endpoint for Solana devnet.


### Observability

The facilitator emits [OpenTelemetry](https://opentelemetry.io)-compatible traces and metrics to standard endpoints,
making it easy to integrate with tools like Honeycomb, Prometheus, Grafana, and others.
Tracing spans are annotated with HTTP method, status code, URI, latency, other request and process metadata.

To enable tracing and metrics export, set the appropriate `OTEL_` environment variables:

```dotenv
# For Honeycomb, for example:
# Endpoint URL for sending OpenTelemetry traces and metrics
OTEL_EXPORTER_OTLP_ENDPOINT=https://api.honeycomb.io:443
# Comma-separated list of key=value pairs to add as headers
OTEL_EXPORTER_OTLP_HEADERS=x-honeycomb-team=your_api_key,x-honeycomb-dataset=x402-facilitator
# Export protocol to use for telemetry. Supported values: `http/protobuf` (default), `grpc`
OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
```

The service automatically detects and initializes exporters if `OTEL_EXPORTER_OTLP_*` variables are provided.

### Supported Networks

The Facilitator supports different networks based on the environment variables you configure:

| Network                   | Environment Variable     | Supported if Set | Notes                            |
|:--------------------------|:-------------------------|:-----------------|:---------------------------------|
| Monad Testnet             | `RPC_URL_MONAD_TESTNET`  | ✅                | Testnet, Recommended for testing |
| Monad Mainnet             | `RPC_URL_MONAD`          | ✅                | Mainnet                          |
| Solana Mainnet            | `RPC_URL_SOLANA`         | ✅                | Mainnet                          |
| Solana Devnet             | `RPC_URL_SOLANA_DEVNET`  | ✅                | Testnet, Recommended for testing |

- If you provide say only `RPC_URL_MONAD_TESTNET`, only **Monad Testnet** will be available.
- If you provide `RPC_URL_MONAD_TESTNET`, `RPC_URL_MONAD`, and other env variables on the list, then all the specified networks will be supported.

> ℹ️ **Tip:** For initial development and testing, you can start with Monad Testnet only.

### Development

Prerequisites:
- Rust 1.80+
- `cargo` and a working toolchain

Build locally:
```shell
cargo build
```
Run:
```shell
cargo run
```

## Related Resources

* [x402 Protocol Documentation](https://x402.org)
* [x402 Overview by Coinbase](https://docs.cdp.coinbase.com/x402/docs/overview)
* [Facilitator Documentation by Coinbase](https://docs.cdp.coinbase.com/x402/docs/facilitator)

## Contributions and feedback welcome!
Feel free to open issues or pull requests to improve x402 support in the Rust ecosystem.

## License

[Apache-2.0](LICENSE)
