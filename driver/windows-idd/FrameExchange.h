// WindowDeck local frame-transfer prototype. Fixed layout shared by x64 processes.
#pragma once
#include <windows.h>
#include <devpropdef.h>
#include <cstddef>
#include <cwchar>

namespace FrameExchange {
inline constexpr DEVPROPKEY MappingProperty = {{0xe93c480a, 0xcc25, 0x4f4d, {0x90,0x32,0x6c,0x21,0x60,0xb1,0x72,0x89}}, 2};
inline constexpr wchar_t Prefix[] = L"Global\\WindowDeck.Frames.";
inline constexpr UINT Width = 1280, Height = 800, Stride = Width * 4;
inline constexpr UINT Bytes = Stride * Height, SlotCount = 3;
inline constexpr UINT Magic = 0x31464457; // WDF1
enum : LONG { Free = 0, Writing = 1, Ready = 2, Reading = 3 };

struct Packet {
    UINT magic, width, height, bytes;
    UINT64 sequence, presentation, acquiredQpc, publishedQpc, frequency, copyMicros;
};
static_assert(sizeof(Packet) == 64);

struct alignas(64) Slot {
    volatile LONG state;
    LONG reserved[15];
    Packet packet;
    BYTE pixels[Bytes];
};
struct alignas(64) Shared {
    UINT magic, version, width, height;
    volatile LONG active, error;
    volatile LONG64 acquired, published, skipped, generation;
    UINT reserved[2];
    Slot slots[SlotCount];
};
static_assert(offsetof(Shared, slots) == 64);
static_assert(sizeof(Shared) == 64 + SlotCount * (128 + Bytes));

inline bool ValidName(PCWSTR name) {
    const size_t prefixLength = ARRAYSIZE(Prefix) - 1;
    if (wcsncmp(name, Prefix, prefixLength) != 0 || wcslen(name) != prefixLength + 36) return false;
    for (size_t i = 0; i < 36; ++i) {
        const wchar_t c = name[prefixLength + i];
        if (i == 8 || i == 13 || i == 18 || i == 23) { if (c != L'-') return false; }
        else if (!((c >= L'0' && c <= L'9') || (c >= L'a' && c <= L'f') || (c >= L'A' && c <= L'F'))) return false;
    }
    return true;
}

// Never wait for a reader. Bounds, offsets and strides are compiled constants,
// not values taken from memory writable by the consumer.
inline Slot* ClaimWriter(Shared* shared, UINT start) {
    for (UINT i = 0; i < SlotCount; ++i) {
        Slot& slot = shared->slots[(start + i) % SlotCount];
        if (InterlockedCompareExchange(&slot.state, Writing, Free) == Free ||
            InterlockedCompareExchange(&slot.state, Writing, Ready) == Ready) return &slot;
    }
    return nullptr;
}
inline void CopyRows(BYTE* destination, const BYTE* source, UINT pitch) {
    for (UINT y = 0; y < Height; ++y) memcpy(destination + y * Stride, source + static_cast<size_t>(y) * pitch, Stride);
}
}
