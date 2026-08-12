# Support-Flow Automation Sketch ($199 → onboarding)

**Target product**: SlidPhi / Lab commercial surface (also reusable pattern for CuNi private workspace later).

## Goal
When a customer pays the $199 support / private-access tier via Stripe, automatically:
1. Send onboarding email with access instructions
2. Create or update a private Notion / Drive note with credentials / next steps
3. (Optional) Grant a temporary access token or invite link

## Minimal viable flow

```
Stripe Checkout / Payment Link ($199)
        ↓
Stripe Webhook (checkout.session.completed or invoice.paid)
        ↓
Server handler (Vercel / Fly / Cloudflare Worker)
        ↓
  ├─ Send email (Resend / Gmail API / Postmark)
  ├─ Write Notion page or Google Drive note
  └─ Log event (for freemium conversion dashboard)
```

## Stripe side
- Product / Price already exists or create one-time $199.
- Use a dedicated Payment Link or Checkout Session with metadata:
  - `tier: support`
  - `product: slidphi` (or cuni)
- Enable webhook endpoint pointing at the handler.

## Handler sketch (pseudo)

```ts
// POST /api/webhooks/stripe
const event = stripe.webhooks.constructEvent(...)
if (event.type === 'checkout.session.completed') {
  const session = event.data.object
  if (session.metadata?.tier === 'support') {
    const email = session.customer_details?.email
    await sendOnboardingEmail(email, {
      accessUrl: 'https://www.slidphilabs.com/access',
      supportNote: 'Private channel + priority response within 24h'
    })
    await createNotionOnboardingPage({
      title: `Support · ${email}`,
      properties: { Status: 'New', Amount: 199 }
    })
  }
}
```

## Onboarding email content (template)
Subject: Welcome — Slid Phi Labs Support Access

Body:
- Thank you
- Link to /access or private workspace
- How to open a support ticket / email
- What is included (response SLA, private notes, etc.)
- Link back to freemium suite for ongoing use

## Notion / notes
- Database: "Support Onboardings" (or reuse Notes for Grok with Status=Support)
- Properties: Email, Paid At, Amount, Status (New / Active / Closed), Notes

## Next concrete steps to ship
1. Confirm existing Stripe $199 product / price ID (or create).
2. Stand up a minimal webhook receiver (can live under Agent-Rider or a small Worker).
3. Wire Resend (or existing Gmail connected tool) for the email.
4. Create the Notion database or page template.
5. Test with Stripe test mode → real $0.00 then live.

## Measurement
- Log every successful onboarding into the freemium conversion note (Step 10).
- Track time-to-first-support-reply separately.
