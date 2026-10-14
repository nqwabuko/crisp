The queue grows because nothing drains it. If the worker stalls, the backlog
doubles. So the alarm fires late. Draining needs the lock, which means writers
wait.

Our team has worked on this for a long time. We have talked to many people
about it. They all had views on the queue. The meetings were long and full of
ideas. We wrote notes after each one. The notes went into a shared folder. The
folder is now quite large. We plan to tidy it next quarter with the whole team.
