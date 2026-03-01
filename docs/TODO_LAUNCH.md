# DevSync Launch TODO (Non-Payment)

## Priority 0 (Must finish before first pilot)

- [ ] Production auth for registry + billing APIs
  - done when API keys (scoped, revocable, expirable) are enforced server-side
- [ ] Org-level access boundaries
  - done when keys can be bound to a single org and cross-org requests are denied
- [ ] Abuse protection
  - done when per-key rate limiting returns HTTP 429 on threshold violations
- [ ] Access observability
  - done when each server writes access logs with actor/key, route, and status
- [ ] Public trust surface
  - done when website includes product, docs links, privacy, terms, and contact

## Priority 1 (First month of pilots)

- [ ] Production deployment topology
  - done when registry and billing services run with backups + restart policy
- [ ] Ops monitoring
  - done when uptime + error alerts are configured
- [x] Entitlement enforcement (org-level active subscription gate)
  - done when registry routes can enforce active subscription per org (`--enforce-entitlements`)
  - next: seat-limit gating by active org members
- [ ] Release process
  - done when tagged binaries and release notes are automated
- [ ] Activation analytics
  - done when funnel metrics are tracked (`init -> up -> activate`)

## Priority 2 (Scale readiness)

- [ ] Hosted multi-tenant control plane
- [ ] SSO/OIDC integration
- [ ] Security review + threat model
- [ ] Data retention and incident runbooks
