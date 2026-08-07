# Insight: operators can't see why DLM throttled a charger

Legacy capacity groups map to circuits in our model. When a charger stops
sending meter values, the platform falls back to a safe current limit to
protect the site. That's the right call. The problem is we never tell the
operator we did it.

So the charger throttles, the driver complains, and the operator has no way to
see the cause. They end up reading comm logs to work out which capacity group
went into fallback. Four customers raised this last quarter.

We should surface fallback state on the capacity group itself. I'd sequence it
with the smart charging work rather than as a one-off.
