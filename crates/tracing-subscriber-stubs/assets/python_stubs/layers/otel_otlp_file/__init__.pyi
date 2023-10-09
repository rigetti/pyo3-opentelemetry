from typing import Optional


class Config:
    """
    A configuration for `opentelemetry-stdout <https://docs.rs/opentelemetry-stdout/latest/opentelemetry_stdout/>`_ layer.
    """

    def __init__(self, *, file_path: Optional[str] = None, filter: Optional[str] = None) -> None:
        """
        :param file_path: The path to the file to write to. If not specified, defaults to stdout.
        :param filter: A filter string to use for this layer. This uses the same format as the
            `tracing_subscriber::filter::EnvFilter <https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html>`_. Shortly, each directive takes the form `target[span{field=value}]=level`, where `target` is roughly the Rust namespace and _only_ `level` is required.

            If not specified, this will first check the `PYO3_TRACING_SUBSCRIBER_ENV_FILTER` environment variable and then `RUST_LOG` environment variable. If all of these values are empty, no spans will be exported. 
        """
        ...

