**The ask**

A general workflow builder for the marketplace, like IFTTT: watch for a trigger, then do something. Halcyon is the first example. When a charger boots, send a DataTransfer, then use the response to update the charger and EVSE records.

# INSIGHT

**Context**

Dana Whitfield (Product Manager, Halcyon Energy) asked on 2026-09-09 that the platform read a charger's real configuration each time it boots, and keep our record in step. Halcyon's residential migration is around 18,000 units, and the drivers moving across want Dynamic Load Management and Power Sharing.

The charger, not the platform, is the source of truth. A driver can change the maximum current with a physical switch on the unit. When our record drifts, load management runs on the wrong numbers and breaks the one feature they migrated for.

**Insight Description**

The platform can tell a customer that something happened on a charger, but cannot act on it. We already ask the charger for its configuration after boot, then send the result out as a webhook. Customers can write values back through the API, and we already run their code as marketplace apps. Nothing joins them up.

So Halcyon's only route is to keep a service running around the clock, catching that webhook and calling us back to write the result. That is the middleware they are paying us to take over.

This request keeps coming back. Brightline Fleet asked in 2025, and Cavendish Motors asked for event-driven throttling. Halcyon named two more of their own uses on the call: credit card handling, and Halcyon Ultra sending user details.

**Significance**

High

Halcyon's residential migration is around 18,000 chargers and gates the second contract. Not Critical, because Halcyon can host the middleware themselves. But that asks them to keep running the system they are replacing.

**Customer Details**
Customer Name(s): Halcyon Energy, Canada
URL (tenant environment): https://halcyon.tenant.example.com/
