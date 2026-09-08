#pragma once
#include "FrameExchange.h"
#include <d3d11_1.h>
#include <dxgi1_6.h>
#include <wrl.h>

// The elevated broker owns the NT handles for one lease. The driver and reader
// open the resources on the same adapter; keys alternate writer=0, reader=1.
struct GpuFrames {
    Microsoft::WRL::ComPtr<ID3D11Texture2D> textures[FrameExchange::SlotCount];
    Microsoft::WRL::ComPtr<IDXGIKeyedMutex> locks[FrameExchange::SlotCount];
    HANDLE handles[FrameExchange::SlotCount] = {};
    ~GpuFrames() { for (HANDLE handle : handles) if (handle) CloseHandle(handle); }
    GpuFrames() = default;
    GpuFrames(const GpuFrames&) = delete;
    GpuFrames& operator=(const GpuFrames&) = delete;
    static D3D11_TEXTURE2D_DESC Description() {
        D3D11_TEXTURE2D_DESC desc = {};
        desc.Width = FrameExchange::Width; desc.Height = FrameExchange::Height;
        desc.MipLevels = desc.ArraySize = desc.SampleDesc.Count = 1;
        desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        desc.Usage = D3D11_USAGE_DEFAULT;
        desc.BindFlags = D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET;
        desc.MiscFlags = D3D11_RESOURCE_MISC_SHARED_NTHANDLE | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX;
        return desc;
    }
    HRESULT Initialize(ID3D11Device* device, PCWSTR mapping, SECURITY_ATTRIBUTES* security = nullptr) {
        if (!FrameExchange::ValidName(mapping)) return E_INVALIDARG;
        Microsoft::WRL::ComPtr<ID3D11Device1> newer;
        HRESULT result = device->QueryInterface(IID_PPV_ARGS(&newer));
        if (FAILED(result)) return result;
        for (UINT i = 0; i < FrameExchange::SlotCount; ++i) {
            wchar_t name[160]; swprintf_s(name, L"%ls.Texture.%u", mapping, i);
            if (security) {
                auto desc = Description();
                result = device->CreateTexture2D(&desc, nullptr, &textures[i]);
                if (FAILED(result)) return result;
                Microsoft::WRL::ComPtr<IDXGIResource1> resource;
                result = textures[i].As(&resource);
                if (FAILED(result)) return result;
                result = resource->CreateSharedHandle(security, DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE, name, &handles[i]);
            } else result = newer->OpenSharedResourceByName(name, DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE, IID_PPV_ARGS(&textures[i]));
            if (FAILED(result)) return result;
            D3D11_TEXTURE2D_DESC desc; textures[i]->GetDesc(&desc);
            const auto expected = Description();
            if (desc.Width != expected.Width || desc.Height != expected.Height || desc.Format != expected.Format ||
                desc.MipLevels != 1 || desc.ArraySize != 1 || desc.SampleDesc.Count != 1 ||
                desc.Usage != expected.Usage || desc.MiscFlags != expected.MiscFlags) return E_INVALIDARG;
            result = textures[i].As(&locks[i]);
            if (FAILED(result)) return result;
        }
        return S_OK;
    }
};

inline HRESULT FrameDevice(LUID luid, ID3D11Device** device) {
    Microsoft::WRL::ComPtr<IDXGIFactory4> factory;
    HRESULT result = CreateDXGIFactory2(0, IID_PPV_ARGS(&factory));
    if (FAILED(result)) return result;
    Microsoft::WRL::ComPtr<IDXGIAdapter1> adapter;
    result = factory->EnumAdapterByLuid(luid, IID_PPV_ARGS(&adapter));
    if (FAILED(result)) return result;
    return D3D11CreateDevice(adapter.Get(), D3D_DRIVER_TYPE_UNKNOWN, nullptr, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        nullptr, 0, D3D11_SDK_VERSION, device, nullptr, nullptr);
}
