# dengjen-tts-grpc

gRPC server frontend for the dengjen-tts speech synthesis engine.

Serves the `DengjenGrpc` service defined in `proto/dengjen_grpc.proto` over `tonic`, loading voices from a config path and streaming synthesized audio back to clients.
