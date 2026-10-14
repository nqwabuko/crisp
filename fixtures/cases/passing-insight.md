**Context**

A fleet operator asked us to read each charger's settings when it boots. Their drivers change the current with a switch on the unit. Our record then drifts from the truth.

**Insight Description**

We can tell them a charger booted, but we cannot act on it. They run a small service to catch our webhook and write the value back. The API takes up to 1000 EVSEs per call, so the write itself is cheap.

**Significance**

High

**Customer Details**

A large residential estate in Canada.
