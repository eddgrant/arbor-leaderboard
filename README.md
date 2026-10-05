# arbor-leaderboard

Reads our children's school meal accounts from the [Arbor](https://arbor-education.com/) parent portal and surfaces them in Home Assistant:

- a Sunday-evening phone notification saying how much to top up each account (to £16)
- dashboard cards for balances, weekly spend and a leaderboard (most puddings, biggest spender, …)

Arbor has no API for parents, so the service uses the same requests as the portal's web pages: a JSON login, then page URLs with `?format=javascript`, which return each page as a JSON component tree.

## Layout

| Path | What |
|---|---|
| `service/` | Rust service: fetches from Arbor, (soon) stores history in SQLite and publishes to MQTT for Home Assistant |
| `deploy/k8s/` | Manifests for Fitlet's k3s cluster (`live` namespace) |
| `homeassistant/` | Home Assistant package (automations, templates) and dashboard YAML |

## Running locally

```sh
cd service
export ARBOR_BASE_URL=https://<school>.uk.arbor.sc
export ARBOR_EMAIL=you@example.com
export ARBOR_PASSWORD="$(op read 'op://<vault>/Arbor/password')"
cargo run --release -- --once
```

`--once` runs a single sync and exits; without it the service keeps syncing every `FETCH_INTERVAL_MINUTES`. Without `MQTT_HOST`, the figures are printed as JSON instead of published.

## Configuration

| Variable | Default | Purpose |
|---|---|---|
| `ARBOR_BASE_URL` | required | The school's Arbor address |
| `ARBOR_EMAIL`, `ARBOR_PASSWORD` | required | Guardian login |
| `DB_PATH` | `arbor.db` | SQLite file holding balances and purchase history |
| `MQTT_HOST` | unset | Broker for Home Assistant; unset prints JSON instead |
| `MQTT_PORT` | `1883` | |
| `MQTT_USERNAME`, `MQTT_PASSWORD` | unset | Broker login, if it needs one |
| `FETCH_INTERVAL_MINUTES` | `120` | Time between syncs |
| `TARGET_BALANCE_PENCE` | `1600` | Balance each account is topped up to |

## Home Assistant entities

Each child appears as a device, "Arbor <name>", with sensors for balance, top-up needed, spend this week and this term, items, puddings and drinks this term, and last purchase date (with that day's items as an `items` attribute). A separate "Arbor leaderboard" device has a "Last successful fetch" timestamp, for alerting when fetches stop working.

## Tests

```sh
cd service && cargo test
```

Parsers are tested against anonymised fixtures in `service/tests/fixtures/` that mirror Arbor's response shapes.
