# Customer Success Playbook (Phase 4)

## Objective
Move teams from trial to habit: weekly environment reuse and fewer onboarding incidents.

## Pilot Lifecycle

1. Kickoff (Day 0)
- Confirm target repositories (3-10 repos)
- Capture baseline metrics:
  - onboarding time
  - drift incidents per month
  - time spent resolving setup issues

2. Enablement (Week 1)
- Run `devsync init`, `devsync policy`, `devsync secret-lint`
- Publish first shared env version (`push`)
- Validate teammate pull workflow (`pull`)

3. Adoption (Week 2-4)
- Require `devsync doctor` + `devsync policy` in PR checks for pilot repos
- Track `registry-audit` events weekly
- Run `devsync activate` on each pilot repo to close gaps

4. Value Review (End of Month 1)
- Run `devsync roi` with real pilot data
- Compare baseline vs current:
  - onboarding time reduction
  - drift incident reduction
  - weekly environment reuse

## Escalation Triggers
- Policy failure rate > 30% after week 2
- No shared environment pulls for 7 days
- >2 unresolved onboarding blockers for a team

## Expansion Criteria
- At least 60% of pilot developers used shared env pull at least once
- At least 40% reduction in onboarding issue resolution time
- Champion identified in engineering leadership
