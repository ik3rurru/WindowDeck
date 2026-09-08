#pragma once
#include <windows.h>

// Fixed phase at 60 Hz. Skip expired slots, never emit a catch-up burst.
struct FrameSchedule {
    UINT64 origin, frequency, index = 0;
    UINT64 Deadline() const { return origin + index * frequency / 60; }
    UINT64 Advance(UINT64 now) {
        ++index;
        const UINT64 firstFuture = (now - origin) * 60 / frequency + 1;
        const UINT64 skipped = firstFuture > index ? firstFuture - index : 0;
        index += skipped;
        return skipped;
    }
};

inline UINT64 FrameClock() {
    LARGE_INTEGER value;
    QueryPerformanceCounter(&value);
    return static_cast<UINT64>(value.QuadPart);
}

struct FrameTimer {
    HANDLE handle = CreateWaitableTimerExW(nullptr, nullptr,
        CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS);
    ~FrameTimer() { if (handle) CloseHandle(handle); }
    bool Wait(UINT64 deadline, UINT64 frequency) const {
        for (;;) {
            const UINT64 now = FrameClock();
            if (now >= deadline) return true;
            LARGE_INTEGER due;
            due.QuadPart = -static_cast<LONGLONG>(((deadline - now) * 10000000 + frequency - 1) / frequency);
            if (!SetWaitableTimer(handle, &due, 0, nullptr, nullptr, FALSE) ||
                WaitForSingleObject(handle, INFINITE) != WAIT_OBJECT_0) return false;
        }
    }
};
