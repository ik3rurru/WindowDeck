// Copyright (c) Microsoft Corporation. Adapted from IddSampleApp; see LICENSE.
#define NOMINMAX
#include <windows.h>
#include <swdevice.h>
#include <cfgmgr32.h>
#include <conio.h>
#include <cstdio>
#include <cwchar>
#include <vector>
#include <string>
#include <sddl.h>
#undef NDEBUG
#include <cassert>
#include "DisplayMode.h"
#include "FrameProbe.h"

struct CreationState
{
    HANDLE completed;
    HRESULT result;
    wchar_t instance[MAX_DEVICE_ID_LEN];
};

static void WINAPI DeviceCreated(HSWDEVICE, HRESULT result, void* context, PCWSTR instance)
{
    auto* state = static_cast<CreationState*>(context);
    state->result = result;
    if (instance) wcscpy_s(state->instance, instance);
    SetEvent(state->completed);
}

static int SelfTest()
{
    FrameExchangeSelfTest();
    unsigned sum = 0;
    for (BYTE byte : DisplayEdid) sum += byte;
    assert((sum & 255) == 0);
    assert(DisplayEdid[0] == 0 && DisplayEdid[1] == 255 && DisplayEdid[126] == 0);
    // Decode the EDID preferred timing and compare it with what IddCx advertises.
    const BYTE* timing = DisplayEdid.data() + 54;
    unsigned width = timing[2] + ((timing[4] & 0xf0) << 4);
    unsigned height = timing[5] + ((timing[7] & 0xf0) << 4);
    unsigned totalWidth = width + timing[3] + ((timing[4] & 15) << 8);
    unsigned totalHeight = height + timing[6] + ((timing[7] & 15) << 8);
    unsigned pixelRate = (timing[0] + (timing[1] << 8)) * 10000;
    assert(width == DisplayWidth && height == DisplayHeight);
    assert(totalWidth == TotalWidth && totalHeight == TotalHeight && pixelRate == PixelRate);
    assert(pixelRate == totalWidth * totalHeight * DisplayRefresh);
    for (bool monitor : {false, true})
    {
        auto signal = DisplaySignal(monitor);
        assert(signal.activeSize.cx == width && signal.activeSize.cy == height);
        assert(signal.totalSize.cx == totalWidth && signal.totalSize.cy == totalHeight);
        assert(signal.pixelRate == pixelRate);
        assert(signal.vSyncFreq.Numerator == 60 && signal.vSyncFreq.Denominator == 1);
        assert(signal.hSyncFreq.Numerator == 60 * totalHeight && signal.hSyncFreq.Denominator == 1);
        assert(signal.AdditionalSignalInfo.vSyncFreqDivider == (monitor ? 0u : 1u));
    }
    CreationState state = {CreateEventW(nullptr, FALSE, FALSE, nullptr), E_PENDING, {}};
    assert(state.completed);
    DeviceCreated(nullptr, E_ACCESSDENIED, &state, nullptr);
    assert(WaitForSingleObject(state.completed, 0) == WAIT_OBJECT_0);
    assert(state.result == E_ACCESSDENIED);
    DeviceCreated(nullptr, S_OK, &state, L"test-instance");
    assert(WaitForSingleObject(state.completed, 0) == WAIT_OBJECT_0);
    assert(state.result == S_OK && wcscmp(state.instance, L"test-instance") == 0);
    CloseHandle(state.completed);
    puts("PASS: EDID, 1280x800@60, creation callbacks, frame slot ownership and padded rows. No device created.");
    return 0;
}

static bool WaitForDriver(PCWSTR instance)
{
    ULONG lastProblem = 0;
    for (int attempt = 0; attempt < 100; ++attempt)
    {
        DEVINST device;
        CONFIGRET found = CM_Locate_DevNodeW(&device, const_cast<PWSTR>(instance), CM_LOCATE_DEVNODE_NORMAL);
        if (found == CR_SUCCESS)
        {
            ULONG status = 0, problem = 0;
            if (CM_Get_DevNode_Status(&status, &problem, device, 0) == CR_SUCCESS)
            {
                lastProblem = problem;
                // Installation may temporarily report a problem before the staged driver starts.
                if ((status & DN_STARTED) && !(status & DN_HAS_PROBLEM)) return true;
            }
        }
        Sleep(100);
    }
    fprintf(stderr, "Driver did not start within 10 seconds (last PnP problem: %lu). Check Device Manager and driver signing.\n", lastProblem);
    return false;
}

static int VerifyDisplay(bool sourceOnly = false, bool quiet = false, RECT* bounds = nullptr)
{
    const UINT32 flags = QDC_ONLY_ACTIVE_PATHS | QDC_VIRTUAL_MODE_AWARE;
    UINT32 pathCount = 0, modeCount = 0;
    LONG result = GetDisplayConfigBufferSizes(flags, &pathCount, &modeCount);
    if (result) { fprintf(stderr, "Display query failed: %ld\n", result); return 1; }
    std::vector<DISPLAYCONFIG_PATH_INFO> paths(pathCount);
    std::vector<DISPLAYCONFIG_MODE_INFO> modes(modeCount);
    result = QueryDisplayConfig(flags, &pathCount, paths.data(), &modeCount, modes.data(), nullptr);
    if (result) { fprintf(stderr, "Display query failed: %ld\n", result); return 1; }
    for (UINT32 i = 0; i < pathCount; ++i)
    {
        const auto& path = paths[i];
        DISPLAYCONFIG_ADAPTER_NAME adapter = {};
        adapter.header = {DISPLAYCONFIG_DEVICE_INFO_GET_ADAPTER_NAME, sizeof(adapter), path.targetInfo.adapterId, 0};
        result = DisplayConfigGetDeviceInfo(&adapter.header);
        if (result) { fprintf(stderr, "Adapter query failed: %ld\n", result); return 1; }
        constexpr wchar_t prefix[] = L"\\\\?\\SWD#WindowDeck#";
        if (_wcsnicmp(adapter.adapterDevicePath, prefix, ARRAYSIZE(prefix) - 1) != 0) continue;
        const UINT32 index = (path.flags & DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE)
            ? path.sourceInfo.sourceModeInfoIdx : path.sourceInfo.modeInfoIdx;
        if (index >= modeCount || modes[index].infoType != DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE) return 1;
        const auto& mode = modes[index].sourceMode;
        if (bounds) *bounds = {mode.position.x, mode.position.y, mode.position.x + static_cast<LONG>(mode.width), mode.position.y + static_cast<LONG>(mode.height)};
        const auto refresh = path.targetInfo.refreshRate;
        const bool valid = mode.width == DisplayWidth && mode.height == DisplayHeight && refresh.Denominator &&
            UINT64{refresh.Numerator} == UINT64{DisplayRefresh} * refresh.Denominator;
        if (sourceOnly)
        {
            if (!valid) { fputs("WindowDeck desktop mode must be 1280x800@60.\n", stderr); return 4; }
            DISPLAYCONFIG_SOURCE_DEVICE_NAME source = {};
            source.header = {DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, sizeof(source), path.sourceInfo.adapterId, path.sourceInfo.id};
            result = DisplayConfigGetDeviceInfo(&source.header);
            if (result) { fprintf(stderr, "Source query failed: %ld\n", result); return 1; }
            wprintf(L"%ls\n", source.viewGdiDeviceName);
            return 0;
        }
        if (!quiet) printf("Active WindowDeck desktop: %ux%u, refresh %u/%u Hz, position %ld,%ld.\n",
            mode.width, mode.height, refresh.Numerator, refresh.Denominator, mode.position.x, mode.position.y);
        return valid ? 0 : 4;
    }
    if (!quiet) fputs("No active WindowDeck desktop screen.\n", stderr);
    return 4;
}

static bool PipeAlive(HANDLE pipe)
{
    DWORD available = 0;
    return PeekNamedPipe(pipe, nullptr, 0, nullptr, &available, nullptr) != FALSE;
}

static int Run(bool probe, HANDLE lease = INVALID_HANDLE_VALUE, FrameMapping* frames = nullptr)
{
    CreationState state = {CreateEventW(nullptr, FALSE, FALSE, nullptr), E_PENDING, {}};
    if (!state.completed)
    {
        fprintf(stderr, "CreateEvent failed: %lu\n", GetLastError());
        return 1;
    }
    SW_DEVICE_CREATE_INFO info = {};
    info.cbSize = sizeof(info);
    info.pszInstanceId = L"WindowDeckDisplay";
    info.pszzHardwareIds = L"WindowDeckDisplay\0";
    info.pszDeviceDescription = L"WindowDeck Display";
    info.CapabilityFlags = SWDeviceCapabilitiesRemovable |
        SWDeviceCapabilitiesSilentInstall | SWDeviceCapabilitiesDriverRequired;
    HSWDEVICE device = nullptr;
    DEVPROPERTY property = {};
    property.CompKey.Key = FrameExchange::MappingProperty;
    property.CompKey.Store = DEVPROP_STORE_SYSTEM;
    property.Type = DEVPROP_TYPE_STRING;
    wchar_t noMapping[] = L"";
    property.BufferSize = sizeof(noMapping);
    property.Buffer = noMapping;
    if (frames) { property.BufferSize = static_cast<ULONG>((wcslen(frames->name) + 1) * sizeof(wchar_t)); property.Buffer = frames->name; }
    HRESULT result = SwDeviceCreate(L"WindowDeck", L"HTREE\\ROOT\\0", &info,
        1, &property, DeviceCreated, &state, &device);
    int exitCode = 1;
    if (FAILED(result))
        fprintf(stderr, "SwDeviceCreate failed: 0x%08lx. Administrator privileges are required; another instance may already be running.\n", static_cast<unsigned long>(result));
    else if (WaitForSingleObject(state.completed, 10000) != WAIT_OBJECT_0)
        fputs("Device creation did not complete within 10 seconds.\n", stderr);
    else if (FAILED(state.result))
        fprintf(stderr, "Device enumeration failed: 0x%08lx\n", static_cast<unsigned long>(state.result));
    else
    {
        wprintf(L"PnP instance: %ls\n", state.instance);
        if (WaitForDriver(state.instance))
        {
            puts("WindowDeck Display driver started. This confirms PnP only, not an active desktop screen.");
            if (lease != INVALID_HANDLE_VALUE)
            {
                const ULONGLONG deadline = GetTickCount64() + 7000;
                while (PipeAlive(lease) && VerifyDisplay(false, true) != 0 && GetTickCount64() < deadline) Sleep(100);
                BYTE ready = VerifyDisplay(false, true) == 0 ? 1 : 0;
                DWORD count = 0;
                if (ready && WriteFile(lease, &ready, 1, &count, nullptr) && count == 1)
                {
                    if (frames && !WriteBytes(lease, frames->name, sizeof(frames->name))) ready = 0;
                    puts("Automatic display active; waiting for lease release.");
                    BYTE release = 0;
                    // EOF also releases the device if the host or its lease process dies.
                    if (ready) ReadFile(lease, &release, 1, &count, nullptr);
                    exitCode = 0;
                }
            }
            else if (probe)
            {
                puts("Probe: checking the active 1280x800@60 desktop and removing the display in five seconds.");
                Sleep(5000);
            }
            else
            {
                puts("Press X to remove the virtual display and exit.");
                int key;
                do { key = _getch(); } while (key != 'x' && key != 'X');
            }
            if (lease == INVALID_HANDLE_VALUE) exitCode = probe ? VerifyDisplay() : 0;
        }
    }
    // SwDeviceClose waits for any creation callback before state/event can be released.
    // Handle lifetime also removes the device if this process is terminated.
    if (device) SwDeviceClose(device);
    if (lease != INVALID_HANDLE_VALUE && device)
    {
        const ULONGLONG deadline = GetTickCount64() + 5000;
        DEVINST node;
        while (CM_Locate_DevNodeW(&node, state.instance, CM_LOCATE_DEVNODE_NORMAL) == CR_SUCCESS &&
            GetTickCount64() < deadline) Sleep(100);
    }
    CloseHandle(state.completed);
    if (!exitCode) puts("Virtual display removal requested; the driver package remains installed.");
    return exitCode;
}

// Restrict the local pipe to the interactive logon, including its UAC-linked token.
// Never accept paths, device IDs or executable commands from the client.
static bool PipeIdentity(std::wstring& name, std::wstring& sddl, std::wstring* logon = nullptr)
{
    HANDLE token = nullptr;
    if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token)) return false;
    DWORD bytes = 0;
    GetTokenInformation(token, TokenLogonSid, nullptr, 0, &bytes);
    std::vector<BYTE> buffer(bytes);
    const bool ok = bytes && GetTokenInformation(token, TokenLogonSid, buffer.data(), bytes, &bytes);
    CloseHandle(token);
    if (!ok) return false;
    auto* groups = reinterpret_cast<TOKEN_GROUPS*>(buffer.data());
    if (groups->GroupCount != 1) return false;
    LPWSTR sid = nullptr;
    if (!ConvertSidToStringSidW(groups->Groups[0].Sid, &sid)) return false;
    name = std::wstring(L"\\\\.\\pipe\\WindowDeck.Display.v1.") + sid;
    // Grant data I/O without FILE_CREATE_PIPE_INSTANCE (part of generic write).
    sddl = std::wstring(L"D:P(A;;0x12019b;;;") + sid + L")S:(ML;;NW;;;ME)";
    if (logon) *logon = sid;
    LocalFree(sid);
    return true;
}

static int Broker(bool frames = false, bool gpuMode = false)
{
    std::wstring name, sddl, logon;
    if (!PipeIdentity(name, sddl, &logon)) return 1;
    if (frames) name += gpuMode ? L".GpuProbe" : L".FrameProbe";
    PSECURITY_DESCRIPTOR descriptor = nullptr;
    if (!ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.c_str(), SDDL_REVISION_1, &descriptor, nullptr)) return 1;
    SECURITY_ATTRIBUTES security = {sizeof(security), descriptor, FALSE};
    HANDLE pipe = CreateNamedPipeW(name.c_str(), PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
        PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, 1, 16, 16, 0, &security);
    LocalFree(descriptor);
    if (pipe == INVALID_HANDLE_VALUE)
    {
        fprintf(stderr, "Cannot start display broker: %lu (another broker may be running).\n", GetLastError());
        return 1;
    }
    puts("WindowDeck broker ready. No monitor until a host session connects. Ctrl+C exits.");
    for (;;)
    {
        if (!ConnectNamedPipe(pipe, nullptr) && GetLastError() != ERROR_PIPE_CONNECTED) break;
        FrameMapping mapping;
        if (!frames || mapping.Create(logon, gpuMode)) Run(false, pipe, frames ? &mapping : nullptr);
        else fprintf(stderr, "Cannot create frame exchange: %lu\n", GetLastError());
        DisconnectNamedPipe(pipe);
    }
    CloseHandle(pipe);
    return 1;
}

static int Lease(bool frames = false, bool gpuMode = false, bool streaming = false, bool metadataOnly = false)
{
    if (streaming) {
        HANDLE watcher = CreateThread(nullptr, 0, WatchStreamHost, nullptr, 0, nullptr);
        if (!watcher) return 1;
        CloseHandle(watcher);
    }
    if (frames) SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    std::wstring name, sddl;
    if (!PipeIdentity(name, sddl)) return 1;
    if (frames) name += gpuMode ? L".GpuProbe" : L".FrameProbe";
    // Identification prevents an impersonating pipe server from acquiring our privileges.
    HANDLE pipe = CreateFileW(name.c_str(), FILE_READ_DATA | FILE_WRITE_DATA | SYNCHRONIZE,
        0, nullptr, OPEN_EXISTING, SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION, nullptr);
    // A forced lease exit can precede asynchronous PnP removal. Allow a short
    // bounded handover, while a second live session still gets ERROR_PIPE_BUSY.
    if (pipe == INVALID_HANDLE_VALUE && GetLastError() == ERROR_PIPE_BUSY && WaitNamedPipeW(name.c_str(), 1000))
        pipe = CreateFileW(name.c_str(), FILE_READ_DATA | FILE_WRITE_DATA | SYNCHRONIZE,
            0, nullptr, OPEN_EXISTING, SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION, nullptr);
    if (pipe == INVALID_HANDLE_VALUE)
    {
        fprintf(stderr, "Display broker unavailable (%lu); start windowdeck-display %s as administrator in this logon.\n", GetLastError(), frames ? (gpuMode ? "--gpu-frame-broker" : "--frame-broker") : "--broker");
        return 1;
    }
    const ULONGLONG deadline = GetTickCount64() + 8000;
    DWORD available = 0;
    while (PeekNamedPipe(pipe, nullptr, 0, nullptr, &available, nullptr) && !available && GetTickCount64() < deadline) Sleep(50);
    BYTE ready = 0;
    DWORD count = 0;
    if (!available || !ReadFile(pipe, &ready, 1, &count, nullptr) || count != 1 || ready != 1)
    {
        fputs("Automatic display did not become ready within eight seconds.\n", stderr);
        CloseHandle(pipe);
        return 1;
    }
    int result = 0;
    if (metadataOnly) {
        wchar_t mapping[128] = {};
        if (!ReadFile(pipe, mapping, sizeof(mapping), &count, nullptr) || count != sizeof(mapping) ||
            mapping[127] || !FrameExchange::ValidName(mapping)) { CloseHandle(pipe); return 1; }
        printf("READY %ls\n", mapping);
    } else if (frames) {
        RECT bounds = {};
        result = VerifyDisplay(false, true, &bounds) == 0 ?
            (streaming ? StreamCpuFrames(pipe) : ReadProbeFrames(pipe, bounds, gpuMode)) : 1;
    } else puts("READY");
    HANDLE input = GetStdHandle(STD_INPUT_HANDLE);
    while ((!frames || metadataOnly) && PipeAlive(pipe) && PeekNamedPipe(input, nullptr, 0, nullptr, &available, nullptr) && !available) Sleep(50);
    BYTE release = 0;
    if (WriteFile(pipe, &release, 1, &count, nullptr))
    {
        const ULONGLONG released = GetTickCount64() + 6000;
        while (PipeAlive(pipe) && GetTickCount64() < released) Sleep(50);
    }
    CloseHandle(pipe);
    return result;
}

int wmain(int argc, wchar_t** argv)
{
    setvbuf(stdout, nullptr, _IONBF, 0);
    if (argc == 2 && wcscmp(argv[1], L"--self-test") == 0) return SelfTest();
    if (argc == 2 && wcscmp(argv[1], L"--verify") == 0) return VerifyDisplay();
    if (argc == 2 && wcscmp(argv[1], L"--source") == 0) return VerifyDisplay(true);
    if (argc == 2 && wcscmp(argv[1], L"--run") == 0) return Run(false);
    if (argc == 2 && wcscmp(argv[1], L"--probe") == 0) return Run(true);
    if (argc == 2 && wcscmp(argv[1], L"--broker") == 0) return Broker();
    if (argc == 2 && wcscmp(argv[1], L"--lease") == 0) return Lease();
    if (argc == 2 && wcscmp(argv[1], L"--gpu-frame-broker") == 0) return Broker(true, true);
    if (argc == 2 && wcscmp(argv[1], L"--gpu-frame-source") == 0) return Lease(true, true);
    if (argc == 2 && wcscmp(argv[1], L"--gpu-lease") == 0) return Lease(true, true, false, true);
    if (argc == 2 && wcscmp(argv[1], L"--version") == 0) { puts("windowdeck-display 0.2.0 frame_protocol=1,2 gpu_lease=true"); return 0; }
    if (argc == 2 && wcscmp(argv[1], L"--frame-broker") == 0) return Broker(true);
    if (argc == 2 && wcscmp(argv[1], L"--frame-source") == 0) return Lease(true);
    if (argc == 2 && wcscmp(argv[1], L"--cpu-frame-stream") == 0) return Lease(true, false, true);
    puts("Usage: windowdeck-display --run | --probe | --verify | --source | --self-test\n--run and --probe require the installed driver and administrator privileges.\n--verify checks an existing active desktop without changing it (0: 1280x800@60, 4: inactive/wrong mode).\n--source prints only the GDI device name of that verified WindowDeck desktop.");
    puts("--broker: elevated local controller; creates a monitor only while --lease holds a connection.\n--lease: host helper; READY on stdout, stdin EOF releases the monitor. Do not run interactively.");
    puts("--gpu-frame-broker / --gpu-frame-source: optional D3D11 shared-texture probe.");
    puts("--gpu-lease: shared-texture lease for the integrated Rust host; READY mapping on stdout, stdin EOF releases the display.");
    puts("--cpu-frame-stream: continuous raw BGRA desktop at 60 FPS for the host; stdin EOF releases the display.");
    puts("--frame-broker: elevated controller for the optional shared-memory frame probe.\n--frame-source: binary BGRA probe helper, launched by windowdeck-host --driver-frame-test.");
    return argc == 1 || (argc == 2 && wcscmp(argv[1], L"--help") == 0) ? 0 : 2;
}
