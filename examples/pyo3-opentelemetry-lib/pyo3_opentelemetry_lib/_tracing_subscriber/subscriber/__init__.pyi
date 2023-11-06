from .. import layers


class Config:
   """
   Configuration for the tracing subscriber. Currently, this only requires a single layer to be
   set on the `tracing_subscriber::Registry`.
   """
   def __init__(self, *, layer: layers.Config):
       ... 
