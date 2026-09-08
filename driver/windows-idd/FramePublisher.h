#pragma once
#include "FrameExchange.h"
#include "GpuFrames.h"

class FramePublisher {
    using Shared = FrameExchange::Shared;
    struct Pending {
        Microsoft::WRL::ComPtr<ID3D11Texture2D> texture;
        bool busy = false;
        UINT64 acquired = 0, presentation = 0;
    } pending[2];
    HANDLE mapping = nullptr;
    Shared* shared = nullptr;
    Microsoft::WRL::ComPtr<ID3D11DeviceContext> gpu;
    ID3D11Device* device;
    UINT cursor = 0;
    UINT64 sequence = 0, frequency = 0;
    GpuFrames textures;
    bool gpuMode = false;
    bool ready = false;
public:
    FramePublisher(ID3D11Device* renderDevice, PCWSTR name) : device(renderDevice) {
        if (!FrameExchange::ValidName(name)) return;
        mapping = OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, FALSE, name);
        if (!mapping) { OutputDebugStringW(L"WindowDeck: frame mapping open failed.\n"); return; }
        shared = static_cast<Shared*>(MapViewOfFile(mapping, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, sizeof(Shared)));
        if (!shared) return;
        gpuMode = shared->version == 2;
        if (shared->magic != FrameExchange::Magic || (shared->version != 1 && !gpuMode)) {
            InterlockedExchange(&shared->error, E_INVALIDARG); return;
        }
        if (gpuMode) {
            const HRESULT result = textures.Initialize(device, name);
            if (FAILED(result)) { InterlockedExchange(&shared->error, result); return; }
            // A swap-chain recreation needs a new lease and fresh keyed mutexes.
            if (InterlockedCompareExchange64(&shared->generation, 1, 0) != 0) {
                InterlockedExchange(&shared->error, E_UNEXPECTED); return;
            }
        } else InterlockedIncrement64(&shared->generation);
        device->GetImmediateContext(&gpu);
        LARGE_INTEGER qpc;
        QueryPerformanceFrequency(&qpc);
        frequency = qpc.QuadPart;
        ready = true;
        InterlockedExchange(&shared->active, 1);
    }
    ~FramePublisher() {
        if (shared) { InterlockedExchange(&shared->active, 0); UnmapViewOfFile(shared); }
        if (mapping) CloseHandle(mapping);
    }
    bool HasPending() const { return pending[0].busy || pending[1].busy; }
    void Submit(IDXGIResource* surface, UINT64 acquired, UINT64 presentation) {
        if (!ready) return;
        InterlockedIncrement64(&shared->acquired);
        Microsoft::WRL::ComPtr<ID3D11Texture2D> source;
        HRESULT result = surface->QueryInterface(IID_PPV_ARGS(&source));
        if (FAILED(result)) { InterlockedExchange(&shared->error, result); return; }
        D3D11_TEXTURE2D_DESC desc;
        source->GetDesc(&desc);
        if (desc.Width != FrameExchange::Width || desc.Height != FrameExchange::Height ||
            desc.MipLevels != 1 || desc.ArraySize != 1 || desc.SampleDesc.Count != 1 ||
            (desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM && desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM_SRGB)) {
            InterlockedExchange(&shared->error, E_INVALIDARG); return;
        }
        if (gpuMode) {
            for (UINT n = 0; n < FrameExchange::SlotCount; ++n) {
                const UINT index = (cursor + n) % FrameExchange::SlotCount;
                auto& slot = shared->slots[index];
                if (InterlockedCompareExchange(&slot.state, FrameExchange::Writing, FrameExchange::Free) != FrameExchange::Free) continue;
                result = textures.locks[index]->AcquireSync(0, 0);
                if (result != S_OK) {
                    InterlockedExchange(&slot.state, FrameExchange::Free);
                    if (result != WAIT_TIMEOUT) { InterlockedExchange(&shared->error, result); ready = false; return; }
                    continue;
                }
                gpu->CopyResource(textures.textures[index].Get(), source.Get());
                gpu->Flush();
                result = textures.locks[index]->ReleaseSync(1);
                if (FAILED(result)) { InterlockedExchange(&shared->error, result); ready = false; return; }
                LARGE_INTEGER published; QueryPerformanceCounter(&published);
                slot.packet = {FrameExchange::Magic, FrameExchange::Width, FrameExchange::Height, FrameExchange::Bytes,
                    ++sequence, presentation, acquired, static_cast<UINT64>(published.QuadPart), frequency, 0};
                InterlockedExchange(&slot.state, FrameExchange::Ready);
                InterlockedIncrement64(&shared->published);
                cursor = (index + 1) % FrameExchange::SlotCount;
                return;
            }
            InterlockedIncrement64(&shared->skipped);
            return;
        }
        for (auto& item : pending) {
            if (item.busy) continue;
            if (!item.texture) {
                desc.Usage = D3D11_USAGE_STAGING;
                desc.BindFlags = 0; desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ; desc.MiscFlags = 0;
                result = device->CreateTexture2D(&desc, nullptr, &item.texture);
                if (FAILED(result)) { InterlockedExchange(&shared->error, result); return; }
            }
            gpu->CopyResource(item.texture.Get(), source.Get());
            // Map(DO_NOT_WAIT) below observes completion without forcing a
            // per-frame immediate-context flush here.
            item.busy = true; item.acquired = acquired; item.presentation = presentation;
            return;
        }
        InterlockedIncrement64(&shared->skipped);
    }
    void Drain() {
        if (!ready || gpuMode) return;
        const UINT first = pending[1].busy && (!pending[0].busy || pending[1].acquired < pending[0].acquired) ? 1 : 0;
        for (UINT i = 0; i < 2; ++i) {
            auto& item = pending[(first + i) % 2];
            if (!item.busy) continue;
            D3D11_MAPPED_SUBRESOURCE mapped = {};
            HRESULT result = gpu->Map(item.texture.Get(), 0, D3D11_MAP_READ, D3D11_MAP_FLAG_DO_NOT_WAIT, &mapped);
            if (result == DXGI_ERROR_WAS_STILL_DRAWING) continue;
            item.busy = false;
            if (FAILED(result)) { InterlockedExchange(&shared->error, result); continue; }
            auto* slot = mapped.RowPitch >= FrameExchange::Stride ? FrameExchange::ClaimWriter(shared, cursor++) : nullptr;
            if (slot) {
                LARGE_INTEGER started, published;
                QueryPerformanceCounter(&started);
                FrameExchange::CopyRows(slot->pixels, static_cast<const BYTE*>(mapped.pData), mapped.RowPitch);
                QueryPerformanceCounter(&published);
                slot->packet = {FrameExchange::Magic, FrameExchange::Width, FrameExchange::Height, FrameExchange::Bytes,
                    ++sequence, item.presentation, item.acquired, static_cast<UINT64>(published.QuadPart), frequency,
                    static_cast<UINT64>((published.QuadPart - started.QuadPart) * 1000000 / frequency)};
                InterlockedExchange(&slot->state, FrameExchange::Ready);
                InterlockedIncrement64(&shared->published);
            } else InterlockedIncrement64(&shared->skipped);
            gpu->Unmap(item.texture.Get(), 0);
        }
    }
};
