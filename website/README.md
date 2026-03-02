# DevSync Marketing Site

Static website for payment-provider onboarding and early GTM.

## Files
- `index.html` - product, pricing, contact, legal links
- `privacy.html` - privacy policy template
- `terms.html` - terms of service template
- `styles.css` - visual system and responsive layout
- `app.js` - year rendering + UX funnel event tracking + lead form handling

## UX Funnel Metrics
The page emits these events into `window.dataLayer`:
- `page_view`
- `cta_click` (for all elements with `data-track`)
- `scroll_depth` at 25/50/75/100%
- `lead_submit` and `lead_submit_invalid`

To forward events to your backend endpoint, set:
- `window.DEVSYNC_ANALYTICS_ENDPOINT = "https://your-domain/events"` before `app.js` loads.

## Current measurable goals
- CTA click-through from hero to pilot section
- Scroll-depth completion through pricing and pilot sections
- Lead form submission rate from pilot section

## Quick publish options

## Vercel
1. Create a new Vercel project from this `website/` directory.
2. Framework preset: `Other`.
3. Output directory: `.`
4. Deploy.

## Netlify
1. Drag-and-drop this `website/` folder in Netlify.
2. Deploy and use generated URL.

## GitHub Pages
1. Push `website/` contents to a repository root.
2. Enable Pages from `main` branch.

## Before submitting to payment onboarding
- Replace placeholder emails (`sales@devsync.dev`, etc.) with real inboxes.
- Replace company details with real legal entity information.
- Keep legal pages linked and accessible.
- Confirm pricing and subscription model text matches your actual billing setup.
