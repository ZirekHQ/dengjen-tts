const PROTO_FILE: &str = "proto/dengjen_grpc.proto";

fn main() {
    if let Err(err) = tonic_build::compile_protos(PROTO_FILE) {
        panic!("failed to compile gRPC protobuf definitions in {PROTO_FILE}: {err:?}");
    }
}
