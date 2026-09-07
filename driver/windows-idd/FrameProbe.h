#pragma once
#include "FrameExchange.h"
#include <objbase.h>
#include <memory>

struct FrameMapping {
    HANDLE handle = nullptr;
    FrameExchange::Shared* memory = nullptr;
    wchar_t name[128] = {};
    ~FrameMapping() { if (memory) UnmapViewOfFile(memory); if (handle) CloseHandle(handle); }
    bool Create(const std::wstring& logon) {
        GUID id;
        wchar_t guid[40];
        if (FAILED(CoCreateGuid(&id)) || !StringFromGUID2(id, guid, ARRAYSIZE(guid))) return false;
        guid[37] = L'\0'; // remove braces
        swprintf_s(name, L"%ls%ls", FrameExchange::Prefix, guid + 1);
        if (!FrameExchange::ValidName(name)) return false;
        // LocalService/UMDF writer and the requesting interactive logon only.
        const auto sddl = std::wstring(L"D:P(A;;GRGW;;;SY)(A;;GRGW;;;LS)(A;;GRGW;;;UD)(A;;GRGW;;;") + logon + L")S:(ML;;NW;;;ME)";
        PSECURITY_DESCRIPTOR descriptor = nullptr;
        if (!ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.c_str(), SDDL_REVISION_1, &descriptor, nullptr)) return false;
        SECURITY_ATTRIBUTES security = {sizeof(security), descriptor, FALSE};
        handle = CreateFileMappingW(INVALID_HANDLE_VALUE, &security, PAGE_READWRITE, 0, sizeof(FrameExchange::Shared), name);
        const DWORD error = GetLastError();
        LocalFree(descriptor);
        if (!handle || error == ERROR_ALREADY_EXISTS) return false;
        memory = static_cast<FrameExchange::Shared*>(MapViewOfFile(handle, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, sizeof(FrameExchange::Shared)));
        if (!memory) return false;
        memory->magic = FrameExchange::Magic; memory->version = 1;
        memory->width = FrameExchange::Width; memory->height = FrameExchange::Height;
        return true;
    }
};

inline void FrameExchangeSelfTest() {
    assert(FrameExchange::ValidName(L"Global\\WindowDeck.Frames.01234567-89ab-cdef-0123-456789abcdef"));
    assert(!FrameExchange::ValidName(L"Local\\WindowDeck.Frames.01234567-89ab-cdef-0123-456789abcdef"));
    assert(!FrameExchange::ValidName(L"Global\\WindowDeck.Frames...\\other"));
    auto memory = std::make_unique<FrameExchange::Shared>();
    for (auto& slot : memory->slots) slot.state = FrameExchange::Reading;
    assert(!FrameExchange::ClaimWriter(memory.get(), 0));
    memory->slots[1].state = FrameExchange::Ready;
    auto* slot = FrameExchange::ClaimWriter(memory.get(), 0);
    assert(slot == &memory->slots[1] && slot->state == FrameExchange::Writing);
    const UINT pitch = FrameExchange::Stride + 32;
    std::vector<BYTE> padded(static_cast<size_t>(pitch) * FrameExchange::Height, 0xee);
    for (UINT y = 0; y < FrameExchange::Height; ++y) memset(padded.data() + y * pitch, y % 251, FrameExchange::Stride);
    FrameExchange::CopyRows(slot->pixels, padded.data(), pitch);
    for (UINT y = 0; y < FrameExchange::Height; ++y)
        for (UINT x = 0; x < FrameExchange::Stride; ++x) assert(slot->pixels[y * FrameExchange::Stride + x] == y % 251);
}

inline LRESULT CALLBACK ProbeWindow(HWND window, UINT message, WPARAM wp, LPARAM lp) {
    if (message == WM_TIMER) { InvalidateRect(window, nullptr, FALSE); return 0; }
    if (message == WM_PAINT) {
        PAINTSTRUCT paint;
        HDC dc = BeginPaint(window, &paint);
        RECT bounds; GetClientRect(window, &bounds);
        HDC buffer = CreateCompatibleDC(dc);
        HBITMAP bitmap = CreateCompatibleBitmap(dc, bounds.right, bounds.bottom);
        if (!buffer || !bitmap) {
            if (buffer) DeleteDC(buffer);
            if (bitmap) DeleteObject(bitmap);
            EndPaint(window, &paint); return 0;
        }
        HGDIOBJ original = SelectObject(buffer, bitmap);
        const unsigned phase = static_cast<unsigned>((GetTickCount64() / 100) % 3);
        const COLORREF colors[] = {RGB(220,40,40), RGB(40,220,40), RGB(40,40,220)};
        HBRUSH brush = CreateSolidBrush(colors[phase]);
        FillRect(buffer, &bounds, brush); DeleteObject(brush);
        RECT white = {0,0,32,32}, black = {1248,0,1280,32};
        FillRect(buffer, &white, static_cast<HBRUSH>(GetStockObject(WHITE_BRUSH)));
        FillRect(buffer, &black, static_cast<HBRUSH>(GetStockObject(BLACK_BRUSH)));
        BitBlt(dc, 0, 0, bounds.right, bounds.bottom, buffer, 0, 0, SRCCOPY);
        SelectObject(buffer, original); DeleteObject(bitmap); DeleteDC(buffer);
        EndPaint(window, &paint);
        return 0;
    }
    return DefWindowProcW(window, message, wp, lp);
}

inline bool WriteBytes(HANDLE output, const void* bytes, DWORD size) {
    const auto* data = static_cast<const BYTE*>(bytes);
    while (size) {
        DWORD written = 0;
        if (!WriteFile(output, data, size, &written, nullptr) || !written) return false;
        size -= written; data += written;
    }
    return true;
}

// Called only after the lease reports the active WindowDeck rectangle.
inline int ReadProbeFrames(HANDLE pipe, const RECT& bounds) {
    wchar_t name[128] = {};
    DWORD count = 0;
    if (!ReadFile(pipe, name, sizeof(name), &count, nullptr) || count != sizeof(name) ||
        name[127] || !FrameExchange::ValidName(name)) return 1;
    HANDLE mapping = OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, FALSE, name);
    if (!mapping) { fprintf(stderr, "Frame consumer cannot open mapping: %lu\n", GetLastError()); return 1; }
    auto* memory = static_cast<FrameExchange::Shared*>(MapViewOfFile(mapping, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, sizeof(FrameExchange::Shared)));
    if (!memory) { CloseHandle(mapping); return 1; }
    WNDCLASSW type = {};
    type.lpfnWndProc = ProbeWindow; type.hInstance = GetModuleHandleW(nullptr); type.lpszClassName = L"WindowDeckFrameProbe";
    RegisterClassW(&type);
    HWND window = CreateWindowExW(WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST, type.lpszClassName,
        L"WindowDeck frame-transfer probe", WS_POPUP, bounds.left, bounds.top,
        FrameExchange::Width, FrameExchange::Height, nullptr, nullptr, type.hInstance, nullptr);
    int result = 1;
    if (window) {
        ShowWindow(window, SW_SHOWNOACTIVATE);
        SetTimer(window, 1, 16, nullptr);
        const ULONGLONG deadline = GetTickCount64() + 15000;
        UINT frames = 0;
        UINT64 previous = 0;
        auto pixels = std::make_unique<BYTE[]>(FrameExchange::Bytes);
        while (frames < 120 && GetTickCount64() < deadline) {
            MSG message;
            while (PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE)) { TranslateMessage(&message); DispatchMessageW(&message); }
            bool delivered = false;
            for (auto& slot : memory->slots) {
                if (InterlockedCompareExchange(&slot.state, FrameExchange::Reading, FrameExchange::Ready) != FrameExchange::Ready) continue;
                const auto packet = slot.packet;
                const bool valid = packet.magic == FrameExchange::Magic && packet.width == FrameExchange::Width &&
                    packet.height == FrameExchange::Height && packet.bytes == FrameExchange::Bytes && packet.sequence > previous;
                if (valid) memcpy(pixels.get(), slot.pixels, FrameExchange::Bytes);
                InterlockedExchange(&slot.state, FrameExchange::Free);
                if (!valid) continue;
                if (!WriteBytes(GetStdHandle(STD_OUTPUT_HANDLE), &packet, sizeof(packet)) ||
                    !WriteBytes(GetStdHandle(STD_OUTPUT_HANDLE), pixels.get(), FrameExchange::Bytes)) goto done;
                previous = packet.sequence; ++frames; delivered = true;
                if (frames == 120) break;
            }
            if (!delivered) Sleep(1);
            DWORD available = 0;
            if (!PeekNamedPipe(pipe, nullptr, 0, nullptr, &available, nullptr)) break;
        }
        result = frames == 120 ? 0 : 1;
done:
        fprintf(stderr, "frame_probe_native frames=%u acquired=%lld published=%lld skipped=%lld active=%ld error=0x%08lx mapping_bytes=%zu\n",
            frames, memory->acquired, memory->published, memory->skipped, memory->active, static_cast<ULONG>(memory->error), sizeof(*memory));
        DestroyWindow(window);
    }
    UnmapViewOfFile(memory); CloseHandle(mapping);
    return result;
}
