# arbor-leaderboard

**Bring your children's school meal accounts into Home Assistant.**

If your children's school uses [Arbor](https://arbor-education.com/) for lunch money, you probably top up each child's account through the Arbor parent portal or app, and only find out what they spent their money on if you go looking. arbor-leaderboard checks Arbor for you and publishes the figures to Home Assistant, so you can:

- **know how much to top up:** a sensor per child shows the top-up needed to bring their balance back to your chosen target. Pair it with a Home Assistant automation for a phone notification before the school week.
- **see what they spend:** balance, spend this week and this term, and what they bought on their last school day.
- **spot problems early:** alert when a balance goes negative, or when the service can't reach Arbor.
- **make it a game:** a family leaderboard for most puddings, most drinks, biggest spender, or any category of food you define.

It runs as a small, self-hosted service (a ~5 MB binary using under 10 MB of memory), keeps the purchase history in a local SQLite database, and talks to Home Assistant over MQTT. Your data stays on your own hardware.

> **Unofficial.** This project isn't affiliated with or endorsed by Arbor Education. Arbor has no public API for parents, so the service signs in with your own parent portal login and reads the same pages the portal shows you. It only reads; it never makes payments. If Arbor changes its portal, the service may stop working until it's updated.

## What you need

- An Arbor **parent portal login** for a school that uses Arbor for meal accounts.
- **Home Assistant** with the [MQTT integration](https://www.home-assistant.io/integrations/mqtt/) set up, and an MQTT broker such as Mosquitto.
- Somewhere to run a container: Docker, Kubernetes, or anything similar.

## Quick start

Find your school's Arbor address: it's the address you see after signing in to the parent portal, for example `https://myschool.uk.arbor.sc`. Then run:

```sh
docker run -d --name arbor-leaderboard \
  -e ARBOR_BASE_URL=https://myschool.uk.arbor.sc \
  -e ARBOR_EMAIL=you@example.com \
  -e ARBOR_PASSWORD='your-arbor-password' \
  -e MQTT_HOST=192.168.1.10 \
  -e TARGET_BALANCE=15 \
  -v arbor-data:/data \
  -p 8080:8080 \
  eddgrant/arbor-leaderboard:latest
```

Within a minute or so, each child appears in Home Assistant under **Settings → Devices & services → MQTT** as a device called "Arbor *child's name*". The first sync reads the whole current term, so it takes longer than later ones.

To try it without Home Assistant, leave out `MQTT_HOST` and add `--once` after the image name: the service syncs once, prints the figures as JSON and exits.

## Home Assistant entities

Each child is a device with these sensors:

| Sensor | Example |
|---|---|
| Balance | £3.40 |
| Top-up needed | £11.60 (to reach `TARGET_BALANCE`) |
| Spend this week, Spend this term | £5.20, £64.55 |
| Items this term | 34 |
| *Category* this term, one per category | Puddings 9, Drinks 5 |
| Last purchase | the date, with that day's items in an `items` attribute |

A separate "Arbor leaderboard" device has a **Last successful fetch** timestamp, so you can be alerted if the service stops being able to read Arbor, for example after a password change.

All entities show as unavailable while the service is stopped.

### Example: a weekly top-up reminder

```yaml
automation:
  - alias: "School lunch top-up reminder"
    triggers:
      - trigger: time
        at: "18:00:00"
    conditions:
      - condition: time
        weekday: [sun]
    actions:
      - action: notify.mobile_app_your_phone
        data:
          title: "School lunch top-ups"
          message: >
            {% for s in states.sensor if s.entity_id is search('^sensor\.arbor_.+_top_up_needed$') %}
            {{ s.name }}: £{{ s.state }}{{ ', ' if not loop.last }}
            {% endfor %}
```

The [`homeassistant/`](homeassistant/) folder will collect ready-made automations and dashboards.

### Categories: puddings, drinks, or anything you like

Each child gets a "*Category* this term" sensor for every item category, counting the items this term whose name contains one of the category's keywords. Till item names are free text ("traybake", "milkshake 1.05"), so keywords are matched case-insensitively anywhere in the name, and an item can count in more than one category.

The built-in categories are **puddings** and **drinks** (see [`service/src/categories.default.yaml`](service/src/categories.default.yaml)). To use your own, write a YAML file and point `CATEGORIES_FILE` at it:

```yaml
categories:
  puddings:
    icon: mdi:cupcake          # optional Material Design icon
    keywords: [traybake, cupcake, cookie, brownie, waffle]
  drinks: [milkshake, slush, water, juice]
  pizza: [pizza]
  healthy:
    icon: mdi:food-apple
    keywords: [fruit, salad, veggie]
```

A category is either a list of keywords or an object with `keywords` and an optional `icon`. Its name becomes the sensor name ("Pizza this term") and entity ID (`sensor.arbor_<child>_pizza_this_term`). Remove a category and its sensors are removed from Home Assistant on the next sync. The service won't start if the file is invalid, and says why.

With Docker, mount the file and set the variable:

```sh
  -v ./categories.yaml:/config/categories.yaml:ro \
  -e CATEGORIES_FILE=/config/categories.yaml \
```

## Configuration

| Variable | Default | Purpose |
|---|---|---|
| `ARBOR_BASE_URL` | required | Your school's Arbor address |
| `ARBOR_EMAIL`, `ARBOR_PASSWORD` | required | Your parent portal login |
| `TARGET_BALANCE` | `16` | The balance, in pounds, you top each account up to, e.g. `15` or `12.50` |
| `MQTT_HOST` | unset | MQTT broker used by Home Assistant; unset prints JSON instead |
| `MQTT_PORT` | `1883` | |
| `MQTT_USERNAME`, `MQTT_PASSWORD` | unset | Broker login, if your broker needs one |
| `DB_PATH` | `/data/arbor.db` in the image, `arbor.db` otherwise | SQLite file holding balances and purchase history; keep it on a persistent volume |
| `FETCH_INTERVAL_MINUTES` | `120` | Time between syncs. Please keep this modest, to be gentle with Arbor |
| `HTTP_PORT` | `8080` | Port for `/health` |
| `CATEGORIES_FILE` | built-in puddings and drinks | YAML file of item categories; see [Categories](#categories-puddings-drinks-or-anything-you-like) |

Treat your Arbor password like any other secret: use your platform's secret store (Docker secrets, Kubernetes Secrets, a password manager's CLI) rather than typing it into shell history.

## Health

`GET /health` returns `200` while the sync loop is making progress and `503` if no sync has finished within the fetch interval plus 15 minutes, which makes it suitable for liveness and readiness probes. Arbor failures, such as a rejected login, don't make it unhealthy, because restarting wouldn't fix them; they appear in the response's `last_error` field, and as a stale **Last successful fetch** in Home Assistant.

## How it works

1. Signs in to Arbor with the same JSON login the parent portal uses.
2. Reads each child's meal account balance from the guardian dashboard.
3. For each day of the current term, reads that day's purchases, skipping days already stored whose total hasn't changed. After the first run, a sync needs only a handful of requests.
4. Calculates each child's figures from the stored history and publishes them to Home Assistant using MQTT discovery, so no Home Assistant YAML is needed for the sensors.

"This term" starts on the first day Arbor lists for the current term. Arbor only shows the current term and resets the list when a new one begins, so term figures read zero until a child's first purchase of the new term, and the local database is what keeps history across terms.

## Repository layout

| Path | What |
|---|---|
| `service/` | The Rust service and its Dockerfile |
| `homeassistant/` | Example Home Assistant automations and dashboards |
| `deploy/k8s/` | Notes for running on Kubernetes |

## Development

```sh
cd service
cargo test
ARBOR_BASE_URL=https://myschool.uk.arbor.sc ARBOR_EMAIL=you@example.com ARBOR_PASSWORD=... \
  cargo run --release -- --once
```

Parsers are tested against anonymised fixtures in `service/tests/fixtures/` that mirror Arbor's response shapes. Please never commit real responses: they contain children's names and purchases.
