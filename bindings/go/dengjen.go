// Package dengjen provides Go bindings for libdengjen, the dengjen-tts C API
// (crates/frontends/capi in the parent repository).
package dengjen

/*
#cgo CFLAGS: -I${SRCDIR}/../../crates/frontends/capi
#cgo LDFLAGS: -L${SRCDIR}/../../target/release -llibdengjen
*/
import "C"
