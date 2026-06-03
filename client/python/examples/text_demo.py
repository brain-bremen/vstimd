"""Quick visual test for the text stimulus — shows text on screen for 3 seconds."""
import time
from vstimd import Connection

with Connection() as conn:
    # White "Hello vstimd" centred on screen
    h = conn.stimuli.create_text(
        text="Hello vstimd",
        x=0, y=50,
        box_width=600, box_height=120,
        letter_height=64,
        r=1.0, g=1.0, b=1.0, a=1.0,
        name="demo_text",
    )
    print(f"created text handle: {h}")
    time.sleep(2)

    # Change text
    conn.stimuli.set_text(h, "Step 6 works!")
    print("updated text")
    time.sleep(2)

    # Change colour to yellow
    conn.stimuli.set_text_color(h, r=1.0, g=1.0, b=0.0)
    print("changed colour to yellow")
    time.sleep(2)

    conn.stimuli.delete(h)
    print("done")
