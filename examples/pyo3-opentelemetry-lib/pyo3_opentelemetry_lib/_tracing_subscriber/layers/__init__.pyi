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

from __future__ import annotations
from typing import TYPE_CHECKING

from . import file as file 
from . import otel_otlp as otel_otlp

if TYPE_CHECKING:
  from typing import Union

  Config = Union[
    file.Config,
    otel_otlp.Config,
      ]
  """
  One of the supported layer configurations that may be set on the subscriber configuration.
  """
