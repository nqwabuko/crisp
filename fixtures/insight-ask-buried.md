# INSIGHT

**Context**
Dana at Halcyon asked for boot-time configuration refresh across 18,000 residential chargers. The platform cannot react to a charger event today without customer-hosted middleware sitting in between.

**Insight Description**
The gap is that `TriggerKindEnum` in `core/modules-marketplace/app-manifest/src/Enums/TriggerKindEnum.php:7-12` has only four cases. Nine listeners hang off `ChargePointBootNotificationReceived`, registered in `core/app/Providers/EventServiceProvider.php:1090`. The write path exists via `ConnectorService::updatePowerOptions` and `$chargePoint->load_group_id`.

**Significance**
High

**Customer Details**
Customer Name(s): Halcyon Energy
