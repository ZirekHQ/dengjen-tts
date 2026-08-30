package dev.dengjen;

/** Mirrors the SYNTH_EVENT_* constants in libdengjen.h. */
public enum EventType {
    SPEECH(0), FINISHED(1), ERROR(2);

    private final int value;

    EventType(int value) {
        this.value = value;
    }

    static EventType fromValue(int value) {
        for (EventType type : values()) {
            if (type.value == value) {
                return type;
            }
        }
        throw new IllegalArgumentException("unknown SynthesisEvent event_type: " + value);
    }
}
