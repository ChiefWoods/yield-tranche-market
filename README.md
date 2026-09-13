# Yield Tranche Market

Yield tranching program with multi-model configuration.

[Source Repository](https://github.com/ChiefWoods/yield-tranche-market)

## How It Works

Tranching turns a yield-bearing asset into two transferable positions:

- **Senior** is the more protected position. It has a claim on the underlying
  strategy and is insulated from Senior-side losses while Junior capital is
  available.
- **Junior** is the first-loss position. It takes Junior-side losses directly
  and absorbs Senior-side losses before they reach Senior. In exchange, it can
  receive a configured share of residual Senior-side yield to compensate for the risk.

When a user deposits the supported yield-bearing asset into a market, the
program places it in a vault and mints
the corresponding Senior or Junior token. To withdraw, a holder burns tranche tokens
and redeems their proportional claim on that tranche's effective NAV.

The market refreshes its accounting by reading directly from the underlying yield source.
Each tranche has a **raw NAV** (custodied balance multiplied by the current
underlying NAV) and an **effective NAV** (the value backing tranche tokens
after the loss waterfall and yield allocation). Junior withdrawals must leave
the market at or above its configured minimum coverage; this keeps enough
Junior capital in place to provide the intended first-loss protection.

### How losses and recoveries work

On a refresh, losses are applied before gains:

1. A loss on Junior's own underlying balance reduces Junior effective NAV.
2. A loss on Senior's underlying balance is first absorbed by Junior effective
   NAV.
3. Only the amount that exceeds Junior effective NAV reduces Senior effective
   NAV.

The program records unrecovered Senior and Junior loss balances. Later gains
must repair those balances before either tranche receives a new yield
allocation. This makes Junior protection meaningful, but it is not a fixed
return or a principal guarantee: Senior can still lose value after the Junior
buffer has been exhausted.

### Tranche models

Every market selects one immutable model at creation. Each model only determines
Junior's share of residual Senior-side gain once loss balances have been repaired.

| Model                        | How Junior's share is determined                                                                                                                                                                |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Point curve**              | A fixed two or three-point curve that linearly interpolates Junior's share from utilization. Useful when the desired payoff schedule is known in advance.                                       |
| **Utilization-guided curve** | An adaptive curve that quotes a share around a target utilization. Its target share adjusts over time within configured limits, making incentives responsive to persistent coverage conditions. |
| **Dynamic leverage**         | Weights Junior effective NAV by a multiplier. The multiplier rises, up to a cap, when the Junior capital ratio falls below its target, increasing Junior's share of residual Senior yield.      |
| **Subsidy**                  | Sends Junior a fixed configured fraction of residual Senior-side gain, while Junior retains its own gains. The simplest option when the intended subsidy is stable.                             |

Higher utilization means Junior coverage is more stretched relative to the
market's minimum coverage requirement. The curve-based models uses that
signal to increase Junior's share of residual Senior yield, compensating it
when protection capital is scarcer.

## Built With

### Languages

- [![Quasar](https://img.shields.io/badge/Quasar-0e0d11?style=for-the-badge)](https://quasar-lang.com/)

## Getting Started

### Prerequisites

1. Update your Solana CLI

```sh
agave-install update
```

2. Install [Just](https://just.systems/)

```sh
brew install just
```

### Setup

> [!TIP]
> Rust nightly is required.

1. Clone the repository

```sh
git clone https://github.com/ChiefWoods/yield-tranche-market.git
```

2. Resync your program id

```sh
cd programs/yield-tranche-market
quasar keys sync
```

3. Build the program

```sh
just build
```

#### Testing

Run all tests.

```sh
just test
```

## Issues

View the [open issues](https://github.com/ChiefWoods/yield-tranche-market/issues) for a full list of proposed features and known bugs.

## Acknowledgements

### Resources

- [Shields.io](https://shields.io/)

## Contact

[chii.yuen@hotmail.com](mailto:chii.yuen@hotmail.com)
