package dev.dengjen;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

class ErrorCodeTest {
    @Test
    void mapsKnownCodes() {
        assertEquals(ErrorCode.NULL_POINTER, ErrorCode.fromCode(22));
        assertEquals(ErrorCode.PANIC, ErrorCode.fromCode(-1));
        assertEquals(ErrorCode.INVALID_HANDLE, ErrorCode.fromCode(-1000));
        assertEquals(ErrorCode.UNSUPPORTED_OPERATION, ErrorCode.fromCode(25));
    }

    @Test
    void fallsBackToUnknownForAnUnrecognizedCode() {
        assertEquals(ErrorCode.UNKNOWN_ERROR, ErrorCode.fromCode(999));
    }

    @Test
    void codeRoundTrips() {
        assertEquals(22, ErrorCode.NULL_POINTER.code());
    }
}
