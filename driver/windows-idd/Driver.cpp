// Copyright (c) Microsoft Corporation.
// Adapted from Windows-driver-samples/video/IndirectDisplay; see LICENSE and README.md.
#define NOMINMAX
#include <windows.h>
#include <wudfwdm.h>
#include <wdf.h>
#include <iddcx.h>
#include <d3d11.h>
#include <dxgi1_6.h>
#include <d3dkmthk.h>
#include <avrt.h>
#include <wrl.h>
#include "DisplayMode.h"
#include "FramePublisher.h"

using Microsoft::WRL::ComPtr;

#include "RenderAdapter.h"

struct DeviceContext { IDDCX_ADAPTER adapter; wchar_t frameMapping[128]; };
WDF_DECLARE_CONTEXT_TYPE(DeviceContext);
struct AdapterContext { wchar_t frameMapping[128]; };
WDF_DECLARE_CONTEXT_TYPE(AdapterContext);

// WDF owns this zero-initialized context; cleanup joins the worker before freeing it.
struct MonitorContext
{
    IDDCX_SWAPCHAIN swapChain;
    LUID renderAdapter;
    HANDLE frameAvailable;
    HANDLE stop;
    HANDLE thread;
    wchar_t frameMapping[128];
};
WDF_DECLARE_CONTEXT_TYPE(MonitorContext);

extern "C" DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD DeviceAdd;
EVT_WDF_DEVICE_D0_ENTRY DeviceD0Entry;
EVT_IDD_CX_ADAPTER_INIT_FINISHED AdapterInitFinished;
EVT_IDD_CX_ADAPTER_COMMIT_MODES CommitModes;
EVT_IDD_CX_PARSE_MONITOR_DESCRIPTION ParseMonitorDescription;
EVT_IDD_CX_MONITOR_GET_DEFAULT_DESCRIPTION_MODES DefaultModes;
EVT_IDD_CX_MONITOR_QUERY_TARGET_MODES TargetModes;
EVT_IDD_CX_MONITOR_ASSIGN_SWAPCHAIN AssignSwapChain;
EVT_IDD_CX_MONITOR_UNASSIGN_SWAPCHAIN UnassignSwapChain;
EVT_WDF_OBJECT_CONTEXT_CLEANUP MonitorCleanup;

static void StopWorker(MonitorContext* context)
{
    if (context->thread)
    {
        SetEvent(context->stop);
        WaitForSingleObject(context->thread, INFINITE);
        CloseHandle(context->thread);
        context->thread = nullptr;
    }
    if (context->stop)
    {
        CloseHandle(context->stop);
        context->stop = nullptr;
    }
}

static HRESULT ConsumeFrames(MonitorContext* context)
{
    ComPtr<IDXGIFactory5> factory;
    HRESULT result = CreateDXGIFactory2(0, IID_PPV_ARGS(&factory));
    if (FAILED(result)) return result;
    ComPtr<IDXGIAdapter1> adapter;
    result = factory->EnumAdapterByLuid(context->renderAdapter, IID_PPV_ARGS(&adapter));
    if (FAILED(result)) return result;
    ComPtr<ID3D11Device> device;
    result = D3D11CreateDevice(adapter.Get(), D3D_DRIVER_TYPE_UNKNOWN, nullptr,
        D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION,
        &device, nullptr, nullptr);
    if (FAILED(result)) return result;
    ComPtr<IDXGIDevice> dxgiDevice;
    result = device.As(&dxgiDevice);
    if (FAILED(result)) return result;
    IDARG_IN_SWAPCHAINSETDEVICE setDevice = {};
    setDevice.pDevice = dxgiDevice.Get();
    result = IddCxSwapChainSetDevice(context->swapChain, &setDevice);
    if (FAILED(result)) return result;

    FramePublisher publisher(device.Get(), context->frameMapping);
    // Check cancellation even when frames arrive continuously.
    while (WaitForSingleObject(context->stop, 0) == WAIT_TIMEOUT)
    {
        publisher.Drain();
        IDARG_OUT_RELEASEANDACQUIREBUFFER buffer = {};
        result = IddCxSwapChainReleaseAndAcquireBuffer(context->swapChain, &buffer);
        if (result == E_PENDING)
        {
            HANDLE events[] = {context->stop, context->frameAvailable};
            DWORD wait = WaitForMultipleObjects(ARRAYSIZE(events), events, FALSE, publisher.HasPending() ? 2 : INFINITE);
            if (wait == WAIT_OBJECT_0) return S_OK;
            if (wait == WAIT_TIMEOUT) continue;
            if (wait != WAIT_OBJECT_0 + 1) return HRESULT_FROM_WIN32(GetLastError());
            continue;
        }
        if (FAILED(result)) return result;
        LARGE_INTEGER acquired;
        QueryPerformanceCounter(&acquired);
        ComPtr<IDXGIResource> surface;
        surface.Attach(buffer.MetaData.pSurface);
        // Queue GPU copy before FinishedProcessingFrame, as required by IddCx.
        // This optional probe does not encode or transmit frames to the Deck.
        publisher.Submit(surface.Get(), acquired.QuadPart, buffer.MetaData.PresentationFrameNumber);
        surface.Reset();
        result = IddCxSwapChainFinishedProcessingFrame(context->swapChain);
        if (FAILED(result)) return result;
        IDARG_IN_REPORTFRAMESTATISTICS report = {};
        auto& statistics = report.FrameStatistics;
        statistics.Size = sizeof(statistics);
        statistics.PresentationFrameNumber = buffer.MetaData.PresentationFrameNumber;
        statistics.FrameStatus = IDDCX_FRAME_STATUS_DROPPED;
        statistics.FrameSliceTotal = 1;
        statistics.FrameAcquireQpcTime = acquired.QuadPart;
        // No bytes are transmitted while this prototype discards frames.
        NTSTATUS status = IddCxSwapChainReportFrameStatistics(context->swapChain, &report);
        if (!NT_SUCCESS(status)) return HRESULT_FROM_NT(status);
    }
    return S_OK;
}

static DWORD WINAPI FrameThread(void* argument)
{
    auto* context = static_cast<MonitorContext*>(argument);
    DWORD task = 0;
    HANDLE scheduling = AvSetMmThreadCharacteristicsW(L"Distribution", &task);
    HRESULT result = ConsumeFrames(context);
    if (FAILED(result)) OutputDebugStringW(L"WindowDeck: swap-chain ended; Windows may recreate it.\n");
    // Returning success from AssignSwapChain transfers ownership to this worker.
    WdfObjectDelete(context->swapChain);
    if (scheduling) AvRevertMmThreadCharacteristics(scheduling);
    return 0;
}

_Use_decl_annotations_
void MonitorCleanup(WDFOBJECT object)
{
    StopWorker(WdfObjectGet_MonitorContext(object));
}

_Use_decl_annotations_
NTSTATUS AssignSwapChain(IDDCX_MONITOR monitor, const IDARG_IN_SETSWAPCHAIN* input)
{
    auto* context = WdfObjectGet_MonitorContext(monitor);
    StopWorker(context);
    context->swapChain = input->hSwapChain;
    context->renderAdapter = input->RenderAdapterLuid;
    context->frameAvailable = input->hNextSurfaceAvailable;
    context->stop = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    if (!context->stop) return STATUS_INSUFFICIENT_RESOURCES;
    context->thread = CreateThread(nullptr, 0, FrameThread, context, 0, nullptr);
    if (!context->thread)
    {
        StopWorker(context);
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS UnassignSwapChain(IDDCX_MONITOR monitor)
{
    StopWorker(WdfObjectGet_MonitorContext(monitor));
    return STATUS_SUCCESS;
}

static IDDCX_MONITOR_MODE MonitorMode(IDDCX_MONITOR_MODE_ORIGIN origin)
{
    IDDCX_MONITOR_MODE mode = {};
    mode.Size = sizeof(mode);
    mode.Origin = origin;
    mode.MonitorVideoSignalInfo = DisplaySignal(true);
    return mode;
}

_Use_decl_annotations_
NTSTATUS ParseMonitorDescription(const IDARG_IN_PARSEMONITORDESCRIPTION* input,
    IDARG_OUT_PARSEMONITORDESCRIPTION* output)
{
    if (input->MonitorDescription.Type != IDDCX_MONITOR_DESCRIPTION_TYPE_EDID ||
        input->MonitorDescription.DataSize != DisplayEdid.size() ||
        !input->MonitorDescription.pData ||
        memcmp(input->MonitorDescription.pData, DisplayEdid.data(), DisplayEdid.size()) != 0)
        return STATUS_INVALID_PARAMETER;
    output->MonitorModeBufferOutputCount = 1;
    output->PreferredMonitorModeIdx = 0;
    if (input->MonitorModeBufferInputCount)
    {
        if (!input->pMonitorModes) return STATUS_INVALID_PARAMETER;
        input->pMonitorModes[0] = MonitorMode(IDDCX_MONITOR_MODE_ORIGIN_MONITORDESCRIPTOR);
    }
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS DefaultModes(IDDCX_MONITOR monitor, const IDARG_IN_GETDEFAULTDESCRIPTIONMODES* input,
    IDARG_OUT_GETDEFAULTDESCRIPTIONMODES* output)
{
    UNREFERENCED_PARAMETER(monitor);
    output->DefaultMonitorModeBufferOutputCount = 1;
    output->PreferredMonitorModeIdx = 0;
    if (input->DefaultMonitorModeBufferInputCount)
    {
        if (!input->pDefaultMonitorModes) return STATUS_INVALID_PARAMETER;
        input->pDefaultMonitorModes[0] = MonitorMode(IDDCX_MONITOR_MODE_ORIGIN_DRIVER);
    }
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS TargetModes(IDDCX_MONITOR monitor, const IDARG_IN_QUERYTARGETMODES* input,
    IDARG_OUT_QUERYTARGETMODES* output)
{
    UNREFERENCED_PARAMETER(monitor);
    output->TargetModeBufferOutputCount = 1;
    if (input->TargetModeBufferInputCount)
    {
        if (!input->pTargetModes) return STATUS_INVALID_PARAMETER;
        IDDCX_TARGET_MODE mode = {};
        mode.Size = sizeof(mode);
        mode.TargetVideoSignalInfo.targetVideoSignalInfo = DisplaySignal(false);
        input->pTargetModes[0] = mode;
    }
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS CommitModes(IDDCX_ADAPTER adapter, const IDARG_IN_COMMITMODES* input)
{
    UNREFERENCED_PARAMETER(adapter);
    UNREFERENCED_PARAMETER(input);
    // No external scan-out hardware to configure; IddCx manages the swap-chain.
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS AdapterInitFinished(IDDCX_ADAPTER adapter, const IDARG_IN_ADAPTER_INIT_FINISHED* input)
{
    if (!NT_SUCCESS(input->AdapterInitStatus)) return input->AdapterInitStatus;
    IDARG_IN_ADAPTERSETRENDERADAPTER preference = {};
    if (SUCCEEDED(PreferredRenderAdapter(preference.PreferredRenderAdapter)))
        IddCxAdapterSetRenderAdapter(adapter, &preference);
    else
        OutputDebugStringW(L"WindowDeck: GPU selection failed; using the Windows default.\n");
    WDF_OBJECT_ATTRIBUTES attributes;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&attributes, MonitorContext);
    attributes.EvtCleanupCallback = MonitorCleanup;
    IDDCX_MONITOR_INFO info = {};
    info.Size = sizeof(info);
    info.MonitorType = DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HDMI;
    info.ConnectorIndex = 0;
    info.MonitorDescription.Size = sizeof(info.MonitorDescription);
    info.MonitorDescription.Type = IDDCX_MONITOR_DESCRIPTION_TYPE_EDID;
    info.MonitorDescription.DataSize = static_cast<UINT>(DisplayEdid.size());
    info.MonitorDescription.pData = const_cast<BYTE*>(DisplayEdid.data());
    // Stable across activate/remove cycles so Windows can retain the desktop arrangement.
    info.MonitorContainerId = {0x5c279a31, 0xc5b0, 0x4e32, {0x9c, 0xaf, 0x37, 0x58, 0x73, 0x40, 0x94, 0x21}};
    IDARG_IN_MONITORCREATE create = {};
    create.ObjectAttributes = &attributes;
    create.pMonitorInfo = &info;
    IDARG_OUT_MONITORCREATE created = {};
    NTSTATUS status = IddCxMonitorCreate(adapter, &create, &created);
    if (!NT_SUCCESS(status)) return status;
    auto* owner = WdfObjectGet_AdapterContext(adapter);
    wcscpy_s(WdfObjectGet_MonitorContext(created.MonitorObject)->frameMapping, owner->frameMapping);
    IDARG_OUT_MONITORARRIVAL arrival = {};
    status = IddCxMonitorArrival(created.MonitorObject, &arrival);
    if (!NT_SUCCESS(status)) WdfObjectDelete(created.MonitorObject);
    return status;
}

_Use_decl_annotations_
NTSTATUS DeviceD0Entry(WDFDEVICE device, WDF_POWER_DEVICE_STATE previousState)
{
    UNREFERENCED_PARAMETER(previousState);
    auto* context = WdfObjectGet_DeviceContext(device);
    if (context->adapter) return STATUS_SUCCESS;
    IDDCX_ADAPTER_CAPS caps = {};
    caps.Size = sizeof(caps);
    caps.MaxMonitorsSupported = 1;
    auto& diagnostics = caps.EndPointDiagnostics;
    diagnostics.Size = sizeof(diagnostics);
    diagnostics.GammaSupport = IDDCX_FEATURE_IMPLEMENTATION_NONE;
    diagnostics.TransmissionType = IDDCX_TRANSMISSION_TYPE_OTHER;
    diagnostics.pEndPointFriendlyName = L"WindowDeck Display";
    diagnostics.pEndPointManufacturerName = L"WindowDeck";
    diagnostics.pEndPointModelName = L"Virtual display prototype";
    IDDCX_ENDPOINT_VERSION version = {};
    version.Size = sizeof(version);
    version.MajorVer = 1;
    diagnostics.pFirmwareVersion = &version;
    diagnostics.pHardwareVersion = &version;
    IDARG_IN_ADAPTER_INIT init = {};
    WDF_OBJECT_ATTRIBUTES adapterAttributes;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&adapterAttributes, AdapterContext);
    init.ObjectAttributes = &adapterAttributes;
    init.WdfDevice = device;
    init.pCaps = &caps;
    IDARG_OUT_ADAPTER_INIT initialized = {};
    NTSTATUS status = IddCxAdapterInitAsync(&init, &initialized);
    if (NT_SUCCESS(status)) {
        context->adapter = initialized.AdapterObject;
        wcscpy_s(WdfObjectGet_AdapterContext(context->adapter)->frameMapping, context->frameMapping);
    }
    return status;
}

_Use_decl_annotations_
NTSTATUS DeviceAdd(WDFDRIVER driver, PWDFDEVICE_INIT deviceInit)
{
    UNREFERENCED_PARAMETER(driver);
    WDF_PNPPOWER_EVENT_CALLBACKS power;
    WDF_PNPPOWER_EVENT_CALLBACKS_INIT(&power);
    power.EvtDeviceD0Entry = DeviceD0Entry;
    WdfDeviceInitSetPnpPowerEventCallbacks(deviceInit, &power);
    IDD_CX_CLIENT_CONFIG config;
    IDD_CX_CLIENT_CONFIG_INIT(&config);
    config.EvtIddCxAdapterInitFinished = AdapterInitFinished;
    config.EvtIddCxAdapterCommitModes = CommitModes;
    config.EvtIddCxParseMonitorDescription = ParseMonitorDescription;
    config.EvtIddCxMonitorGetDefaultDescriptionModes = DefaultModes;
    config.EvtIddCxMonitorQueryTargetModes = TargetModes;
    config.EvtIddCxMonitorAssignSwapChain = AssignSwapChain;
    config.EvtIddCxMonitorUnassignSwapChain = UnassignSwapChain;
    NTSTATUS status = IddCxDeviceInitConfig(deviceInit, &config);
    if (!NT_SUCCESS(status)) return status;
    WDF_OBJECT_ATTRIBUTES attributes;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&attributes, DeviceContext);
    WDFDEVICE device = nullptr;
    status = WdfDeviceCreate(&deviceInit, &attributes, &device);
    if (!NT_SUCCESS(status)) return status;
    WDF_DEVICE_PROPERTY_DATA property;
    WDF_DEVICE_PROPERTY_DATA_INIT(&property, &FrameExchange::MappingProperty);
    ULONG needed = 0;
    DEVPROPTYPE type = 0;
    auto* context = WdfObjectGet_DeviceContext(device);
    const NTSTATUS queried = WdfDeviceQueryPropertyEx(device, &property, sizeof(context->frameMapping),
        context->frameMapping, &needed, &type);
    if (!NT_SUCCESS(queried) || type != DEVPROP_TYPE_STRING || needed < sizeof(wchar_t) ||
        needed > sizeof(context->frameMapping) || needed % sizeof(wchar_t) ||
        context->frameMapping[needed / sizeof(wchar_t) - 1] != L'\0' || !FrameExchange::ValidName(context->frameMapping))
        context->frameMapping[0] = L'\0';
    return IddCxDeviceInitialize(device);
}

_Use_decl_annotations_
extern "C" NTSTATUS DriverEntry(PDRIVER_OBJECT driver, PUNICODE_STRING registryPath)
{
    WDF_DRIVER_CONFIG config;
    WDF_DRIVER_CONFIG_INIT(&config, DeviceAdd);
    return WdfDriverCreate(driver, registryPath, WDF_NO_OBJECT_ATTRIBUTES, &config, WDF_NO_HANDLE);
}
