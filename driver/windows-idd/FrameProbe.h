#pragma once
#include "FrameExchange.h"
#include "FramePacing.h"
#include "GpuFrames.h"
#include "RenderAdapter.h"
#include <objbase.h>
#include <memory>

struct FrameMapping {
    HANDLE handle = nullptr;
    FrameExchange::Shared* memory = nullptr;
    wchar_t name[128] = {};
    GpuFrames textures;
    Microsoft::WRL::ComPtr<ID3D11Device> gpuDevice;
    ~FrameMapping() { if (memory) UnmapViewOfFile(memory); if (handle) CloseHandle(handle); }
    bool Create(const std::wstring& logon, bool gpuMode = false) {
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
        HRESULT gpuResult = S_OK;
        LUID adapter = {};
        if (handle && error != ERROR_ALREADY_EXISTS && gpuMode) {
            gpuResult = PreferredRenderAdapter(adapter);
            if (SUCCEEDED(gpuResult)) gpuResult = FrameDevice(adapter, &gpuDevice);
            if (SUCCEEDED(gpuResult)) gpuResult = textures.Initialize(gpuDevice.Get(), name, &security);
        }
        LocalFree(descriptor);
        if (FAILED(gpuResult)) { fprintf(stderr, "GPU frame resources failed: 0x%08lx\n", static_cast<ULONG>(gpuResult)); return false; }
        if (!handle || error == ERROR_ALREADY_EXISTS) return false;
        memory = static_cast<FrameExchange::Shared*>(MapViewOfFile(handle, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, sizeof(FrameExchange::Shared)));
        if (!memory) return false;
        memory->magic = FrameExchange::Magic; memory->version = gpuMode ? 2 : 1;
        memcpy(memory->reserved, &adapter, sizeof(adapter));
        memory->width = FrameExchange::Width; memory->height = FrameExchange::Height;
        return true;
    }
};

inline void FrameExchangeSelfTest() {
    FrameSchedule schedule{0, 60000};
    assert(schedule.Deadline() == 0);
    assert(schedule.Advance(100) == 0 && schedule.Deadline() == 1000);
    assert(schedule.Advance(3500) == 2 && schedule.Deadline() == 4000);
    assert(schedule.Advance(4100) == 0 && schedule.Deadline() == 5000);
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

// Continuous CPU input for libx264. One current frame, no unbounded queue.
// stdin belongs exclusively to the host; EOF must also interrupt a blocked stdout.
inline DWORD WINAPI WatchStreamHost(void*) {
    BYTE byte;
    DWORD count;
    ReadFile(GetStdHandle(STD_INPUT_HANDLE), &byte, 1, &count, nullptr);
    ExitProcess(0);
}

inline int StreamCpuFrames(HANDLE pipe) {
    wchar_t name[128] = {};
    DWORD count = 0;
    if (!ReadFile(pipe, name, sizeof(name), &count, nullptr) || count != sizeof(name) ||
        name[127] || !FrameExchange::ValidName(name)) return 1;
    HANDLE mapping = OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, FALSE, name);
    if (!mapping) return 1;
    auto* memory = static_cast<FrameExchange::Shared*>(MapViewOfFile(mapping,
        FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, sizeof(FrameExchange::Shared)));
    if (!memory) { CloseHandle(mapping); return 1; }
    int result = 1;
    if (memory->magic == FrameExchange::Magic && memory->version == 1) {
        auto pixels = std::make_unique<BYTE[]>(FrameExchange::Bytes);
        UINT64 previous = 0, outputs = 0, fresh = 0, writeMicros = 0, maxWriteMicros = 0;
        LARGE_INTEGER clockFrequency;
        QueryPerformanceFrequency(&clockFrequency);
        const UINT64 frequency = clockFrequency.QuadPart;
        FrameTimer timer;
        if (!timer.handle) { UnmapViewOfFile(memory); CloseHandle(mapping); return 1; }
        const UINT64 started = FrameClock();
        FrameSchedule schedule{started, frequency};
        UINT64 report = started, reportOutputs = 0, skipped = 0;
        UINT64 intervalWriteUs = 0, intervalMaxWriteUs = 0;
        while (InterlockedCompareExchange(&memory->error, 0, 0) == 0) {
            DWORD available = 0;
            if (!PeekNamedPipe(pipe, nullptr, 0, nullptr, &available, nullptr) || available) break;
            if (!timer.Wait(schedule.Deadline(), frequency)) break;
            for (auto& slot : memory->slots) {
                if (InterlockedCompareExchange(&slot.state, FrameExchange::Reading, FrameExchange::Ready) != FrameExchange::Ready) continue;
                const auto packet = slot.packet;
                const bool valid = packet.magic == FrameExchange::Magic && packet.width == FrameExchange::Width &&
                    packet.height == FrameExchange::Height && packet.bytes == FrameExchange::Bytes && packet.sequence > previous;
                if (valid) { memcpy(pixels.get(), slot.pixels, FrameExchange::Bytes); previous = packet.sequence; ++fresh; }
                InterlockedExchange(&slot.state, FrameExchange::Free);
            }
            if (!previous) {
                if (FrameClock() - started > frequency * 8) break;
                Sleep(1); continue;
            }
            // Repeat the latest desktop for CFR, including a completely static desktop.
            const UINT64 writeStarted = FrameClock();
            if (!WriteBytes(GetStdHandle(STD_OUTPUT_HANDLE), pixels.get(), FrameExchange::Bytes)) break;
            const UINT64 now = FrameClock();
            const UINT64 writeUs = (now - writeStarted) * 1000000 / frequency;
            intervalWriteUs += writeUs; intervalMaxWriteUs = std::max(intervalMaxWriteUs, writeUs);
            writeMicros += writeUs; maxWriteMicros = std::max(maxWriteMicros, writeUs);
            ++outputs;
            skipped += schedule.Advance(now);
            if (now - report >= frequency) {
                fprintf(stderr, "driver_cpu_stream outputs=%llu fresh=%llu last_sequence=%llu elapsed_ms=%llu write_mean_us=%llu write_max_us=%llu\n",
                    outputs, fresh, previous, (now - started) * 1000 / frequency, outputs ? writeMicros / outputs : 0, maxWriteMicros);
                const UINT64 intervalOutputs = outputs - reportOutputs;
                fprintf(stderr, "driver_cpu_interval fps=%.3f write_mean_us=%llu write_max_us=%llu missed_deadlines=%llu\n",
                    intervalOutputs * static_cast<double>(frequency) / (now - report),
                    intervalOutputs ? intervalWriteUs / intervalOutputs : 0, intervalMaxWriteUs, skipped);
                reportOutputs = outputs; intervalWriteUs = intervalMaxWriteUs = skipped = 0;
                report = now;
            }
        }
    }
    UnmapViewOfFile(memory); CloseHandle(mapping);
    return result;
}

// Called only after the lease reports the active WindowDeck rectangle.
inline int ReadProbeFrames(HANDLE pipe, const RECT& bounds, bool gpuMode = false) {
    wchar_t name[128] = {};
    DWORD count = 0;
    if (!ReadFile(pipe, name, sizeof(name), &count, nullptr) || count != sizeof(name) ||
        name[127] || !FrameExchange::ValidName(name)) return 1;
    HANDLE mapping = OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, FALSE, name);
    if (!mapping) { fprintf(stderr, "Frame consumer cannot open mapping: %lu\n", GetLastError()); return 1; }
    auto* memory = static_cast<FrameExchange::Shared*>(MapViewOfFile(mapping, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, sizeof(FrameExchange::Shared)));
    if (!memory) { CloseHandle(mapping); return 1; }
    Microsoft::WRL::ComPtr<ID3D11Device> device;
    Microsoft::WRL::ComPtr<ID3D11DeviceContext> context;
    Microsoft::WRL::ComPtr<ID3D11Texture2D> staging;
    GpuFrames textures;
    HRESULT setup = memory->magic == FrameExchange::Magic && memory->version == (gpuMode ? 2u : 1u) ? S_OK : E_INVALIDARG;
    if (SUCCEEDED(setup) && gpuMode) {
        LUID adapter; memcpy(&adapter, memory->reserved, sizeof(adapter));
        setup = FrameDevice(adapter, &device);
        if (SUCCEEDED(setup)) setup = textures.Initialize(device.Get(), name);
        if (SUCCEEDED(setup)) {
            device->GetImmediateContext(&context);
            auto desc = GpuFrames::Description();
            desc.Usage = D3D11_USAGE_STAGING; desc.BindFlags = desc.MiscFlags = 0; desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
            setup = device->CreateTexture2D(&desc, nullptr, &staging);
        }
    }
    if (FAILED(setup)) {
        fprintf(stderr, "Frame reader setup failed: 0x%08lx\n", static_cast<ULONG>(setup));
        UnmapViewOfFile(memory); CloseHandle(mapping); return 1;
    }
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
        double acquireToCpuMs = 0, readbackMs = 0;
        auto pixels = std::make_unique<BYTE[]>(FrameExchange::Bytes);
        while (frames < 120 && GetTickCount64() < deadline) {
            if (InterlockedCompareExchange(&memory->error, 0, 0) != 0) break;
            MSG message;
            while (PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE)) { TranslateMessage(&message); DispatchMessageW(&message); }
            bool delivered = false;
            for (auto& slot : memory->slots) {
                if (InterlockedCompareExchange(&slot.state, FrameExchange::Reading, FrameExchange::Ready) != FrameExchange::Ready) continue;
                const auto packet = slot.packet;
                const bool valid = packet.magic == FrameExchange::Magic && packet.width == FrameExchange::Width &&
                    packet.height == FrameExchange::Height && packet.bytes == FrameExchange::Bytes && packet.sequence > previous;
                LARGE_INTEGER started, finished; QueryPerformanceCounter(&started);
                if (gpuMode) {
                    const UINT index = static_cast<UINT>(&slot - memory->slots);
                    HRESULT hr = textures.locks[index]->AcquireSync(1, 0);
                    if (hr == WAIT_TIMEOUT) { InterlockedExchange(&slot.state, FrameExchange::Ready); continue; }
                    if (hr != S_OK) { InterlockedExchange(&memory->error, hr); goto done; }
                    // Always consume the key, even when dropping an older packet.
                    if (valid) context->CopyResource(staging.Get(), textures.textures[index].Get());
                    context->Flush();
                    hr = textures.locks[index]->ReleaseSync(0);
                    if (FAILED(hr)) { InterlockedExchange(&memory->error, hr); goto done; }
                    if (valid) {
                        D3D11_MAPPED_SUBRESOURCE mapped = {};
                        do {
                            hr = context->Map(staging.Get(), 0, D3D11_MAP_READ, D3D11_MAP_FLAG_DO_NOT_WAIT, &mapped);
                            if (hr != DXGI_ERROR_WAS_STILL_DRAWING) break;
                            Sleep(1);
                        } while (GetTickCount64() < deadline);
                        if (FAILED(hr)) { InterlockedExchange(&memory->error, hr); goto done; }
                        if (mapped.RowPitch < FrameExchange::Stride) { context->Unmap(staging.Get(), 0); goto done; }
                        FrameExchange::CopyRows(pixels.get(), static_cast<const BYTE*>(mapped.pData), mapped.RowPitch);
                        context->Unmap(staging.Get(), 0);
                    }
                } else if (valid) memcpy(pixels.get(), slot.pixels, FrameExchange::Bytes);
                QueryPerformanceCounter(&finished);
                InterlockedExchange(&slot.state, FrameExchange::Free);
                if (!valid) continue;
                if (!packet.frequency || packet.acquiredQpc > static_cast<UINT64>(finished.QuadPart)) goto done;
                acquireToCpuMs += (finished.QuadPart - packet.acquiredQpc) * 1000.0 / packet.frequency;
                readbackMs += (finished.QuadPart - started.QuadPart) * 1000.0 / packet.frequency;
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
        fprintf(stderr, "frame_probe_transfer mode=%s frames=%u acquire_to_reader_cpu_mean_ms=%.3f reader_copy_mean_ms=%.3f\n",
            gpuMode ? "d3d11" : "cpu", frames, frames ? acquireToCpuMs / frames : 0, frames ? readbackMs / frames : 0);
        fprintf(stderr, "frame_probe_native frames=%u acquired=%lld published=%lld skipped=%lld active=%ld error=0x%08lx mapping_bytes=%zu\n",
            frames, memory->acquired, memory->published, memory->skipped, memory->active, static_cast<ULONG>(memory->error), sizeof(*memory));
        DestroyWindow(window);
    }
    UnmapViewOfFile(memory); CloseHandle(mapping);
    return result;
}
