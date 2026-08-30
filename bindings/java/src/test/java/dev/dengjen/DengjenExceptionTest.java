package dev.dengjen;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

class DengjenExceptionTest {
    @Test
    void usesTheGivenMessageWhenPresent() {
        var ex = new DengjenException(ErrorCode.NULL_POINTER, "voice_ptr was null");
        assertEquals("voice_ptr was null", ex.getMessage());
        assertEquals(ErrorCode.NULL_POINTER, ex.errorCode());
    }

    @Test
    void fallsBackToADescriptiveMessageWhenNoneIsGiven() {
        var ex = new DengjenException(ErrorCode.UNKNOWN_ERROR, "");
        assertEquals("UNKNOWN_ERROR (no message from libdengjen)", ex.getMessage());
    }

    @Test
    void fallsBackToADescriptiveMessageWhenMessageIsNull() {
        var ex = new DengjenException(ErrorCode.UNKNOWN_ERROR, null);
        assertEquals("UNKNOWN_ERROR (no message from libdengjen)", ex.getMessage());
    }
}
