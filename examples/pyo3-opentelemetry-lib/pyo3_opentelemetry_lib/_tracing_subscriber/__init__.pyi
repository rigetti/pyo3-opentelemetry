from types import TracebackType
from typing import Optional, Type, Union
from . import subscriber as subscriber
from . import layers as layers


class TracingContextManagerError(RuntimeError):
    ...


class TracingInitializationError(RuntimeError):
    ...


class TracingStartError(RuntimeError):
    ...


class TracingShutdownError(RuntimeError):
    ...


class BatchConfig:
    def __init__(self, *, subscriber: subscriber.Config):
        ... 


class SimpleConfig:
    def __init__(self, *, subscriber: subscriber.Config):
        ...


ExportConfig = Union[BatchConfig, SimpleConfig]
TracingConfig = Union[CurrentThreadTracingConfig, GlobalTracingConfig]


class CurrentThreadTracingConfig:
    def __init__(self, *, export_process: ExportConfig):
        ... 


class GlobalTracingConfig:
    def __init__(self, *, export_process: ExportConfig):
        ... 


class Tracing:
    def __init__(self, *, config: TracingConfig):
        ...

    async def __aenter__(self):
        ... 

    async def __aexit__(self, exc_type: Optional[Type[BaseException]], exc_value: Optional[BaseException], traceback: Optional[TracebackType]):
        ...

