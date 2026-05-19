from __future__ import annotations


class VstimdError(Exception):
    """Base class for all errors returned by vstimd."""


class HandleNotFoundError(VstimdError):
    """The stimulus handle does not exist on the server."""


class WrongStimulusTypeError(VstimdError):
    """The command is not applicable to this stimulus type."""


class WrongTargetError(VstimdError):
    """A system command was sent with a stimulus handle, or vice versa."""


class CreationFailedError(VstimdError):
    """The server could not create the stimulus (resource exhaustion, etc.)."""


class InvalidArgumentError(VstimdError):
    """A field value is out of range or logically invalid."""


class NotSupportedError(VstimdError):
    """Command exists but is not supported in the current configuration."""


class UnknownServerError(VstimdError):
    """Unexpected server-side error."""


class NotReadyError(VstimdError):
    """Server is still initialising; retry after the first rendered frame."""
