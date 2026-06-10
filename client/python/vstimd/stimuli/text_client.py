from __future__ import annotations

from typing import Callable

from vstimd._handles import StimulusHandle
from vstimd._proto import service_pb2
from vstimd._proto.vstimd.v1 import vec2_pb2, color_pb2
from vstimd._proto.vstimd.v1.stimuli import text_pb2

from .color import Color
from .text_models import LanguageStyle, _LANGUAGE_STYLE_TO_PROTO
from .vec import Vec2

_SendFn = Callable[[service_pb2.Request], service_pb2.Response]


class TextClient:
    """Create and mutate text stimuli."""

    def __init__(self, send: _SendFn) -> None:
        self._send = send

    def create_text(
        self,
        *,
        text: str = "",
        pos: Vec2 = Vec2(0.0, 0.0),
        box_width: float = 400.0,
        box_height: float = 100.0,
        letter_height: float = 32.0,
        font: str = "",
        anchor: str = "center",
        color: Color = Color(1.0, 1.0, 1.0),
        fill_color: Color = Color(0.0, 0.0, 0.0, 0.0),
        language_style: LanguageStyle = LanguageStyle.LTR,
        name: str = "",
        id: str = "",
    ) -> StimulusHandle:
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            create_text=text_pb2.CreateTextRequest(
                text=text,
                font=font,
                letter_height=letter_height,
                size=vec2_pb2.Vec2(x=box_width, y=box_height),
                pos=vec2_pb2.Vec2(x=pos.x, y=pos.y),
                anchor=anchor,
                color=color_pb2.Color(r=color.r, g=color.g, b=color.b, a=color.a),
                fill_color=color_pb2.Color(
                    r=fill_color.r, g=fill_color.g, b=fill_color.b, a=fill_color.a
                ),
                language_style=_LANGUAGE_STYLE_TO_PROTO[language_style],
                name=name,
                id=id,
            ),
        )
        return StimulusHandle(self._send(req).handle)

    def set_text(self, handle: StimulusHandle, text: str) -> None:
        self._send(service_pb2.Request(
            stimulus=handle,
            set_text=text_pb2.SetTextRequest(text=text),
        ))

    def set_text_color(self, handle: StimulusHandle, color: Color) -> None:
        self._send(service_pb2.Request(
            stimulus=handle,
            set_text_color=text_pb2.SetTextColorRequest(
                color=color_pb2.Color(r=color.r, g=color.g, b=color.b, a=color.a),
            ),
        ))
