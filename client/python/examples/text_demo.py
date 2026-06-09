"""Quick visual test for the text stimulus — shows text on screen for 3 seconds."""
import time
from vstimd import Connection
from vstimd.stimuli.stimuli_models import Color, Vec2

with Connection() as conn:
    # White "Hello vstimd" centred on screen
    h = conn.stimuli.create_text(
        text="Hello vstimd",
        pos=Vec2(0, 50),
        box_width=600, box_height=120,
        letter_height=64,
        color=Color(1.0, 1.0, 1.0),
        name="demo_text",
    )
    print(f"created text handle: {h}")
    time.sleep(2)

    # Change text
    conn.stimuli.set_text(h, "Step 6 works!")
    print("updated text")
    time.sleep(2)

    # Change colour to yellow
    conn.stimuli.set_text_color(h, Color(1.0, 1.0, 0.0))
    print("changed colour to yellow")
    time.sleep(2)

    conn.stimuli.delete(h)
    print("done")
