from {{ host_package }}.{{ tracing_subscriber_module_name }}.layers import otel_file


__doc__ = otel_file.__doc__
__all__ = getattr(otel_file, "__all__", [])

