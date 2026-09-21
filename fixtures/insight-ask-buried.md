# INSIGHT

**Context**
Martin at FLO asked for boot-time configuration refresh across 25,000 residential chargers. The platform cannot react to a charger event today without customer-hosted middleware sitting in between.

**Insight Description**
The gap is that `ScriptTriggerEnum` in `main/modules-marketplace/app-manifest/src/Enums/ScriptTriggerEnum.php:7-12` has only four cases. Nine listeners hang off `ChargePointBootNotificationReceived`, registered in `main/app/Providers/EventServiceProvider.php:1090`. The write path exists via `EvseService::updatePowerOptions` and `$chargePoint->dlm_circuit_id`.

**Significance**
High

**Customer Details**
Customer Name(s): FLO
