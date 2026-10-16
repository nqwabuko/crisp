# Why the radiator overshoots

A thermostat loop has three parts: a sensor, a controller and a boiler. The
sensor reads the room, the controller compares it with the setting, and the
boiler adds heat when the room is cold.

You might wonder why the room ends up warmer than the setting. The answer is
delay. The boiler heats water, the water heats the radiator, and only then does
the radiator heat the air, so the sensor learns about each burst of heat
minutes after it was paid for.

It works in two steps. First, the controller keeps the boiler on until the
sensor says the room is warm enough. Second, the heat already stored in the
water and the radiator keeps arriving after the boiler stops, which means the
room carries on warming past the setting. That stored heat is the overshoot:
the loop acted on old news, because news of the room always arrives late.
A slower sensor makes it worse, because the loop then waits even longer before
it hears that the room is warm. A faster sensor helps, but it cannot remove the
heat that is already stored in the water, so some overshoot always remains while
the radiator keeps giving heat to the air.
