# Billing API (Phase 4 Slice)

## Overview
DevSync includes a file-backed billing backend with a local HTTP API surface.

Start server:

```bash
devsync billing-serve --bind 127.0.0.1:8795
```

Optional auth:

```bash
devsync billing-serve --bind 127.0.0.1:8795 --auth-token "$DEVSYNC_AUTH_TOKEN"
```

All routes are `POST` and JSON request/response.

Remote CLI mode:

```bash
devsync billing-plan-ls --billing-url http://127.0.0.1:8795 --auth-token "$DEVSYNC_AUTH_TOKEN"
devsync billing-subscribe acme --plan team --seats 5 --billing-url http://127.0.0.1:8795 --auth-token "$DEVSYNC_AUTH_TOKEN"
```

If auth is enabled, send:

```http
Authorization: Bearer <token>
```

## Routes

### `POST /v1/billing/plans/list`
Request:

```json
{}
```

Response: array of plan objects.

### `POST /v1/billing/subscriptions/create`
Request:

```json
{
  "org": "acme",
  "plan": "team",
  "seats": 5,
  "customer_email": "billing@acme.com"
}
```

Response: subscription object.

### `POST /v1/billing/subscriptions/list`
Request:

```json
{ "org": "acme" }
```

Response: array of subscription objects.

### `POST /v1/billing/cycle/run`
Request:

```json
{ "at": "2099-01-01T00:00:00Z" }
```

Response:

```json
{
  "effective_at": "2099-01-01T00:00:00+00:00",
  "invoices_created": 1,
  "events_created": 1
}
```

### `POST /v1/billing/invoices/list`
Request:

```json
{ "org": "acme" }
```

Response: array of invoice objects.

### `POST /v1/billing/invoices/pay`
Request:

```json
{ "invoice_id": "inv_..." }
```

Response: updated invoice object.

### `POST /v1/billing/events/list`
Request:

```json
{
  "org": "acme",
  "pending_only": true
}
```

Response: array of event objects.

### `POST /v1/billing/events/ack`
Request:

```json
{ "event_id": "evt_..." }
```

Response: updated event object (`delivered_at` set).

## Notes
- Default billing store root: `~/.devsync/billing`
- Data file: `store.toml`
- Default plans are seeded automatically: `team`, `business`, `enterprise`
- Event outbox is webhook-ready: use `billing-events` + `billing-event-ack` for delivery workflow
