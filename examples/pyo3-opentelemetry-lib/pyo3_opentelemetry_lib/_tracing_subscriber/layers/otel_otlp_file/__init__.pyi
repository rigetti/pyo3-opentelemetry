# *****************************************************************************
# *                             AUTO-GENERATED CODE                           *
# *                                                                           *
# * This code was generated by the `pyo3-tracing-subscriber` crate. Any       *
# * modifications to this file should be made to the script or the generation *
# * process that produced this code. Specifically, see:                       *
# * `pyo3_tracing_subscriber::stubs::write_stub_files`                        *
# *                                                                           *
# * Do not manually edit this file, as your changes may be overwritten the    *
# * next time the code is generated.                                          *
# *****************************************************************************

from typing import Optional, final
from pyo3_opentelemetry_lib._tracing_subscriber.common import InstrumentationLibrary


@final
class Config:
    """
    A configuration for `opentelemetry-stdout <https://docs.rs/opentelemetry-stdout/latest/opentelemetry_stdout/>`_
    layer.
    """

    def __new__(cls, *, file_path: Optional[str] = None, filter: Optional[str] = None, instrumentation_library: Optional[InstrumentationLibrary] = None) -> "Config":
        """
        :param file_path: The path to the file to write to. If not specified, defaults to stdout.
        :param filter: A filter string to use for this layer. This uses the same format as the
            `tracing_subscriber::filter::EnvFilter
            <https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html>`_.
            In summary, each directive takes the form `target[span{field=value}]=level`, where `target` is
            roughly the Rust namespace and _only_ `level` is required.

            If not specified, this will first check the `PYO3_TRACING_SUBSCRIBER_ENV_FILTER` environment variable
            and then `RUST_LOG` environment variable. If all of these values are empty, no spans will be exported.
        :param instrumentation_library: Information about the library providing the tracing instrumentation.
        """
        ...

