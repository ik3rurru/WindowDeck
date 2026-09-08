#pragma once
#include <d3dkmthk.h>
#include <dxgi1_6.h>
#include <wrl.h>
using Microsoft::WRL::ComPtr;
// ponytail: prefer the low-power GPU for this prototype; tune this preference for other GPU combinations.
constexpr auto RenderPreference = DXGI_GPU_PREFERENCE_MINIMUM_POWER;

inline HRESULT PreferredRenderAdapter(LUID& luid)
{
    ComPtr<IDXGIFactory6> factory;
    HRESULT result = CreateDXGIFactory2(0, IID_PPV_ARGS(&factory));
    if (FAILED(result)) return result;
    for (UINT index = 0; ; ++index)
    {
        ComPtr<IDXGIAdapter1> adapter;
        result = factory->EnumAdapterByGpuPreference(index, RenderPreference, IID_PPV_ARGS(&adapter));
        if (FAILED(result)) return result;
        DXGI_ADAPTER_DESC1 description = {};
        result = adapter->GetDesc1(&description);
        if (FAILED(result)) return result;
        D3DKMT_OPENADAPTERFROMLUID opened = {description.AdapterLuid, 0};
        if (D3DKMTOpenAdapterFromLuid(&opened) < 0) continue;
        D3DKMT_ADAPTERTYPE type = {};
        D3DKMT_QUERYADAPTERINFO query = {opened.hAdapter, KMTQAITYPE_ADAPTERTYPE, &type, sizeof(type)};
        NTSTATUS status = D3DKMTQueryAdapterInfo(&query);
        D3DKMT_CLOSEADAPTER close = {opened.hAdapter};
        NTSTATUS closed = D3DKMTCloseAdapter(&close);
        if (closed < 0) return HRESULT_FROM_NT(closed);
        // DXGI can expose this IDD under its render GPU's name; only select real render adapters.
        if (status >= 0 && type.RenderSupported && !type.IndirectDisplayDevice && !type.SoftwareDevice)
        {
            luid = description.AdapterLuid;
            return S_OK;
        }
    }
}
