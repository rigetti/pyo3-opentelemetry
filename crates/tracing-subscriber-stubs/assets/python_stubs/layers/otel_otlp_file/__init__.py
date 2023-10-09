from {{ host_package }}.{{ tracing_subscriber_module_name }}.layers import otel_otlp_file


__doc__ = otel_otlp_file.__doc__
__all__ = getattr(otel_otlp_file, "__all__", [])

