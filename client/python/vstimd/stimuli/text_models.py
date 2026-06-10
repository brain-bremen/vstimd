from __future__ import annotations

from enum import Enum

from vstimd._proto.vstimd.v1.stimuli import text_pb2


class LanguageStyle(Enum):
    LTR = "LTR"
    RTL = "RTL"
    ARABIC = "Arabic"


_LANGUAGE_STYLE_TO_PROTO: dict[LanguageStyle, text_pb2.LanguageStyle] = {
    LanguageStyle.LTR:    text_pb2.LANGUAGE_STYLE_LTR,
    LanguageStyle.RTL:    text_pb2.LANGUAGE_STYLE_RTL,
    LanguageStyle.ARABIC: text_pb2.LANGUAGE_STYLE_ARABIC,
}
