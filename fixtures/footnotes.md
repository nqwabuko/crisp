# Footnotes

A charger on a cellular link uses very little data. OCPP messages are small, and
a heartbeat is about fifty bytes of JSON.[^ft-1] Take a busy charger running
dozens of sessions a day: it uses tens of megabytes a month.[^ft-2] Prior to the
switch to WebSockets, the same traffic cost noticeably more.

Most operators buy plans from specialist IoT providers.[^ft-3] It is worth noting
that a thousand chargers at three pounds a month is a real line in the budget.

[^ft-1]: Open Charge Alliance, OCPP 1.6J specification, section 4, on message
    framing; and the OCA's own notes, prior to the 2.0.1 release, on the reduction
    in mobile data costs after moving to JSON over WebSockets.
[^ft-2]: An illustrative estimate, with heartbeats every 60 seconds and meter values every 30 seconds while charging, TLS and TCP overhead included.
[^ft-3]: 1NCE, product page (accessed October 2026), on ten years of service with 500 MB for a one-off fee.
