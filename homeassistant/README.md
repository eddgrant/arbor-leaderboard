# Home Assistant

- `packages/`: automations and template sensors (e.g. the Sunday 18:00 top-up notification)
- `dashboards/`: dashboard YAML for balances, spend and the leaderboard

The entities come from the service via MQTT discovery, so no sensor YAML is needed for the raw values.
