from types import TracebackType
from typing import Optional, Type, Union
from . import subscriber as subscriber
from . import layers as layers


class TracingContextManagerError(RuntimeError):
    """
    Raised if the initialization, enter, and exit of the tracing context manager was
    invoked in an invalid order.
    """
    ...


class TracingStartError(RuntimeError):
    """
    Raised if the tracing subscriber configuration is invalid or if a background export task
    fails to start.
    """
    ...


class TracingShutdownError(RuntimeError):
    """
    Raised if the tracing subscriber fails to shutdown cleanly.
    """
    ...


class BatchConfig:
    """
    Configuration for exporting spans in batch. This will require a background task to be spawned
    and run for the duration of the tracing context manager.

    This configuration is typically favorable unless the tracing context manager is short lived.
    """
    def __init__(self, *, subscriber: subscriber.Config):
        ... 


class SimpleConfig:
    """
    Configuration for exporting spans in a simple manner. This does not spawn a background task
    unless it is required by the configured export layer. Generally favor `BatchConfig` instead,
    unless the tracing context manager is short lived.

    Note, some export layers still spawn a background task even when `SimpleConfig` is used. 
    This is the case for the OTLP export layer, which makes gRPC export requests within the
    background Tokio runtime.
    """
    def __init__(self, *, subscriber: subscriber.Config):
        ...


ExportConfig = Union[BatchConfig, SimpleConfig]
"""
One of `BatchConfig` or `SimpleConfig`.
"""


class CurrentThreadTracingConfig:
    """
    This tracing configuration will export spans emitted only on the current thread. A `Tracing` context
    manager may be initialized multiple times for the same process with this configuration (although
    they should not be nested).

    Note, this configuration is currently incompatible with async methods defined with `pyo3_asyncio`.
    """
    def __init__(self, *, export_process: ExportConfig):
        ... 


class GlobalTracingConfig:
    """
    This tracing configuration will export spans emitted on any thread in the current process. Because
    it sets a tracing subscriber at the global level, it can only be initialized once per process.

    This is typically favorable, as it only requires a single initialization across your entire Python
    application.
    """
    def __init__(self, *, export_process: ExportConfig):
        ... 


TracingConfig = Union[CurrentThreadTracingConfig, GlobalTracingConfig]
"""
One of `CurrentThreadTracingConfig` or `GlobalTracingConfig`.
"""


class Tracing:
    """
    An asynchronous context manager that initializes a tracing subscriber and exports spans
    emitted from within the parent Rust-Python package.

    Each instance of this context manager can only be used once and only once.
    """
    def __init__(self, *, config: TracingConfig):
        ...

    async def __aenter__(self):
        ... 

    async def __aexit__(self, exc_type: Optional[Type[BaseException]], exc_value: Optional[BaseException], traceback: Optional[TracebackType]):
        ...

