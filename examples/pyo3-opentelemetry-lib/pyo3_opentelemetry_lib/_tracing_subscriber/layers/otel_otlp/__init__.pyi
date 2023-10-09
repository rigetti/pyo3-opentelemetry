from typing import Dict, List, Optional, Union

class SpanLimits:
    def __init__(
        self,
        *,
        max_events_per_span: Optional[int] = None,
        max_attributes_per_span: Optional[int] = None,
        max_links_per_span: Optional[int] = None,
        max_attributes_per_event: Optional[int] = None,
        max_attributes_per_link: Optional[int] = None,
    ) -> None: ...
    """

    :param max_events_per_span: The max events that can be added to a `Span`.
    :param max_attributes_per_span: The max attributes that can be added to a `Span`.
    :param max_links_per_span: The max links that can be added to a `Span`.
    :param max_attributes_per_event: The max attributes that can be added to an `Event`.
    :param max_attributes_per_link: The max attributes that can be added to a `Link`.
    """

ResourceValueArray = Union[List[bool], List[int], List[float], List[str]]
"""
An array of `ResourceValue`s. This array is homogenous, so all values must be of the same type.
"""

ResourceValue = Union[bool, int, float, str, ResourceValueArray]
"""
A value that can be added to a `Resource`.
"""

class Resource:
    """
    A `Resource` is a representation of the entity producing telemetry. This should represent the Python
    process starting the tracing subscription process.
    """

    def __init__(
        self,
        *,
        attrs: Optional[Dict[str, ResourceValue]] = None,
        schema_url: Optional[str] = None,
    ) -> None: ...

Sampler = Union[bool, float]
"""
A `Sampler` is a representation of the sampling strategy to use. If this is a `bool`, it will
either sample all traces (`True`) or none of them (`False`). If this is a `float`, it will sample
traces at the given rate.
"""

class Config:
    """
    A configuration for `opentelemetry-otlp <https://docs.rs/opentelemetry-otlp/latest/opentelemetry_otlp/>`_
    layer. In addition to the values specified at initialization, this configuration will also respect the
    canonical `OpenTelemetry OTLP environment variables
    <https://opentelemetry.io/docs/specs/otel/protocol/exporter/>`_ that are `supported by opentelemetry-otlp
    <https://docs.rs/opentelemetry-otlp/latest/opentelemetry_otlp/trait.WithExportConfig.html#tymethod.with_env>`_.
    """

    def __init__(
        self,
        *,
        span_limits: Optional[SpanLimits] = None,
        resource: Optional[Resource] = None,
        metadata_map: Optional[Dict[str, str]] = None,
        sampler: Optional[Sampler] = None,
        endpoint: Optional[str] = None,
        timeout_millis: Optional[int] = None,
        pre_shutdown_timeout_millis: Optional[int] = 2000,
        filter: Optional[str] = None,
    ) -> None:
        """
        Initializes a new `Config`.

        :param span_limits: The limits to apply to span exports.
        :param resource: The OpenTelemetry resource to attach to all exported spans.
        :param metadata_map: A map of metadata to attach to all exported spans. This is a map of key value pairs
            that may be set as gRPC metadata by the tonic library.
        :param sampler: The sampling strategy to use. See documentation for `Sampler` for more information.
        :param endpoint: The endpoint to export to. This should be a valid URL. If not specified, this should be
            specified by environment variables (see `Config` documentation).
        :param timeout_millis: The timeout for each request, in milliseconds. If not specified, this should be
            specified by environment variables (see `Config` documentation).
        :param pre_shutdown_timeout_millis: The timeout to wait before shutting down the OTLP exporter in milliseconds.
            This timeout is necessary to ensure all traces from `tracing_subscriber` to make it to the OpenTelemetry
            layer, which may be effectively force flushed. It is enforced on the `Tracing` context manager exit.
        :param filter: A filter string to use for this layer. This uses the same format as the
            `tracing_subscriber::filter::EnvFilter
            <https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html>`_.
            In summary, each directive takes the form `target[span{field=value}]=level`, where `target` is roughly the
            Rust namespace and _only_ `level` is required.

            If not specified, this will first check the `PYO3_TRACING_SUBSCRIBER_ENV_FILTER` environment variable
            and then `RUST_LOG` environment variable. If all of these values are empty, no spans will be exported.
        """
        ...
