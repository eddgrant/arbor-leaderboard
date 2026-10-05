# Running on Kubernetes

The service needs:

- **one replica** with the `Recreate` deployment strategy, since SQLite expects a single writer
- a **persistent volume** mounted at `/data` for the database
- a **Secret** for `ARBOR_EMAIL`, `ARBOR_PASSWORD` and, if needed, the MQTT login
- liveness and readiness probes on `GET /health`, port 8080

Example manifests will be added here.
