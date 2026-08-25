# Padding and redundancy

It should be noted that the depot rollout was signed off prior to the
deadline. At the end of the day, the end result is that we can basically plan
ahead for phase two, which is absolutely essential.

Needless to say, the past history here is quite complex. It is worth noting that
the two teams collaborated together in close proximity to go-live, and the
migration script was really just a very simple change to `tariff_id`.

The customer said, "needless to say, it broke" — and it did. As such, we reverted
back and completely eliminated the retry loop:

```python
# padding words inside code must survive: it should be noted that basically
result = very_simple(x)
```

That said, the added bonus is a brief summary in the console, so obviously the
operator can see which limit actually applied.
