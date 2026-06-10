from __future__ import annotations

from vstimd.stimuli.color import Color


class ServerVersion:
    """Semver triple reported by the server."""

    def __init__(self, major: int, minor: int, patch: int) -> None:
        self.major = major
        self.minor = minor
        self.patch = patch

    def __repr__(self) -> str:
        return f"ServerVersion({self.major}, {self.minor}, {self.patch})"

    def __str__(self) -> str:
        return f"{self.major}.{self.minor}.{self.patch}"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, ServerVersion):
            return NotImplemented
        return (self.major, self.minor, self.patch) == (other.major, other.minor, other.patch)

    def __lt__(self, other: "ServerVersion") -> bool:
        return (self.major, self.minor, self.patch) < (other.major, other.minor, other.patch)


class ServerInfo:
    """Display and version information returned by :meth:`SystemClient.query_server_info`."""

    def __init__(
        self,
        width: int,
        height: int,
        frame_rate: float,
        version: ServerVersion,
        background_color: Color = Color(0.0, 0.0, 0.0),
    ) -> None:
        self.width = width
        self.height = height
        self.frame_rate = frame_rate
        self.version = version
        self.background_color = background_color

    def __repr__(self) -> str:
        return (
            f"ServerInfo(width={self.width}, height={self.height}, "
            f"frame_rate={self.frame_rate:.1f}, version={self.version})"
        )
