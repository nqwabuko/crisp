# Insight: DLM capability gaps

Prior to the commencement of the migration, it was determined that there are a
number of limitations in relation to the current implementation of the dynamic
load management functionality, which, due to the fact that capacity groups are
modelled as circuits in the legacy platform, necessitates additional
configurability in terms of how meter values are utilised for the purposes of
load balancing across the estate.

In the event that the aforementioned meter values are not received on a regular
basis, the allocation of available current is calculated by the platform using a
fallback which was implemented for the purpose of protecting the site, and it
should be noted that this behaviour is not currently surfaced to the operator in
a manner which facilitates rapid diagnosis.

It may be the case that the prioritisation of visibility improvements would
deliver a more holistic outcome. There is a possibility that alignment with the
smart charging workstream is required.
