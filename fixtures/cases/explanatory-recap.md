# Why the radiator overshoots

A thermostat loop has three parts: a sensor, a controller and a boiler. The
sensor reads the room, the controller compares it with the setting, and the
boiler adds heat when the room is cold.

You might wonder why the room ends up warmer than the setting. The answer is
delay. The boiler heats water, the water heats the radiator, and only then does
the radiator heat the air, so the sensor learns about each burst of heat
minutes after it was paid for.

It works in two steps. First, the controller keeps the boiler on until the
sensor says the room is warm enough. Second, the heat already in the water and
the radiator keeps arriving after the boiler stops, which means the room
carries on warming past the setting.


The sensor reads the room, and the controller compares it with the setting. The boiler adds heat to the room. A thermostat loop has a sensor, a controller and a boiler. The sensor reads the room and the boiler heats the room.
