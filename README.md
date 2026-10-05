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
cargo run --release > snapshot.json
```

This prints a JSON snapshot: each child's balance, the top-up needed, and every purchase this term.

## Tests

```sh
cd service && cargo test
```

Parsers are tested against anonymised fixtures in `service/tests/fixtures/` that mirror Arbor's response shapes.
