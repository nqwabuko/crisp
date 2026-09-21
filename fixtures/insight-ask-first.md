**The ask**

A general workflow builder for the marketplace, like IFTTT: watch for a trigger, then do something. FLO is the first example. When a charger boots, send a DataTransfer, then use the response to update the charger and EVSE records.

# INSIGHT

**Context**

Martin Faisant (Product Manager, FLO) asked on 2026-09-09 that AMPECO read a charger's real configuration each time it boots, and keep our record in step. FLO's residential migration is around 25,000 units, and the drivers moving across want Dynamic Load Management and Power Sharing.

The charger, not the platform, is the source of truth. A driver can change the maximum current with a physical switch on the unit. When our record drifts, load management runs on the wrong numbers and breaks the one feature they migrated for.

**Insight Description**

AMPECO can tell a customer that something happened on a charger, but cannot act on it. We already ask the charger for its configuration after boot, then send the result out as a webhook. Customers can write values back through the API, and we already run their code as marketplace apps. Nothing joins them up.

So FLO's only route is to keep a service running around the clock, catching that webhook and calling us back to write the result. That is the middleware they are paying us to take over.

This request keeps coming back. JLR asked in 2025, and AGL asked for event-driven throttling. FLO named two more of their own uses on the call: credit card handling, and FLO Ultra sending user details.

**Significance**

High

FLO's residential migration is around 25,000 chargers and gates the second contract. Not Critical, because FLO can host the middleware themselves. But that asks them to keep running the system they are replacing.

**Customer Details**
Customer Name(s): FLO (AddÉnergie Technologies), Canada
URL (tenant environment): https://flo.ca-flo.charge.ampeco.tech/
