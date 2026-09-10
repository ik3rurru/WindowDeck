#include "common.h"
#include "../../../driver/windows-idd/GpuFrames.h"
#include "../../../driver/windows-idd/FramePacing.h"
#include "../../../driver/windows-idd/RenderAdapter.h"
extern "C" {
#include <libavutil/hwcontext_d3d11va.h>
}
#include <algorithm>
#include <map>

using Microsoft::WRL::ComPtr;
namespace {
void Hr(HRESULT result, const char* operation) {
    if (SUCCEEDED(result)) return;
    char code[32]; snprintf(code, sizeof(code), "0x%08lx", static_cast<unsigned long>(result));
    throw std::runtime_error(std::string(operation) + ": " + code);
}
struct Mapping {
    HANDLE handle = nullptr;
    FrameExchange::Shared* memory = nullptr;
    ~Mapping() { if (memory) UnmapViewOfFile(memory); if (handle) CloseHandle(handle); }
};
struct Encoder {
    Mapping mapping;
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    ComPtr<ID3D11VideoDevice> videoDevice;
    ComPtr<ID3D11VideoContext> videoContext;
    ComPtr<ID3D11VideoProcessorEnumerator> enumerator;
    ComPtr<ID3D11VideoProcessor> processor;
    ComPtr<ID3D11Texture2D> latest;
    GpuFrames shared;
    BufferPtr hardware, frames;
    CodecPtr codec;
    PacketPtr packet{av_packet_alloc()};
    ScalePtr scaler;
    FrameExchange::Packet latestPacket{};
    std::map<int64_t, std::pair<uint64_t, uint64_t>> pending;
    uint64_t frequency = 0, lastSequence = 0, emittedSequence = 0, number = 0;
    FrameTimer timer;
    FrameSchedule schedule{};
    uint64_t fresh = 0, repeated = 0, stale = 0, converted = 0, conversionUs = 0, encoded = 0, encodeUs = 0, capturePublishUs = 0;
    uint64_t report = Micros(), started = Micros();
    bool software = false, synthetic = false;

    explicit Encoder(const char* mappingName, const char* requested, bool test = false) : synthetic(test) {
        if (!packet) throw std::bad_alloc();
        LARGE_INTEGER clock;
        QueryPerformanceFrequency(&clock); frequency = static_cast<uint64_t>(clock.QuadPart);
        if (!timer.handle) throw std::runtime_error("high-resolution frame timer unavailable");
        schedule = {FrameClock(), frequency};
        LUID luid{};
        if (synthetic) {
            Hr(PreferredRenderAdapter(luid), "select the same adapter as the display broker");
            Hr(FrameDevice(luid, &device), "create diagnostic D3D11 device");
        } else {
        const std::string name(mappingName);
        const std::wstring wide(name.begin(), name.end());
        if (!FrameExchange::ValidName(wide.c_str())) throw std::runtime_error("invalid GPU mapping name");
        mapping.handle = OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, FALSE, wide.c_str());
        if (!mapping.handle) throw std::runtime_error("cannot open GPU mapping");
        mapping.memory = static_cast<FrameExchange::Shared*>(MapViewOfFile(mapping.handle,
            FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, sizeof(FrameExchange::Shared)));
        if (!mapping.memory || mapping.memory->magic != FrameExchange::Magic || mapping.memory->version != 2)
            throw std::runtime_error("GPU mapping requires driver protocol 2");
        memcpy(&luid, mapping.memory->reserved, sizeof(luid));
        Hr(FrameDevice(luid, &device), "create capture D3D11 device");
        Hr(shared.Initialize(device.Get(), wide.c_str()), "open shared GPU surfaces");
        }
        device->GetImmediateContext(&context);
        Hr(device.As(&videoDevice), "query D3D11 video device");
        Hr(context.As(&videoContext), "query D3D11 video context");
        auto description = GpuFrames::Description();
        description.MiscFlags = 0;
        Hr(device->CreateTexture2D(&description, nullptr, &latest), "create private capture surface");
        D3D11_VIDEO_PROCESSOR_CONTENT_DESC content{};
        content.InputFrameFormat = D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE;
        content.InputWidth = content.OutputWidth = FrameExchange::Width;
        content.InputHeight = content.OutputHeight = FrameExchange::Height;
        content.InputFrameRate = content.OutputFrameRate = {60, 1};
        content.Usage = D3D11_VIDEO_USAGE_OPTIMAL_SPEED;
        Hr(videoDevice->CreateVideoProcessorEnumerator(&content, &enumerator), "create GPU converter enumerator");
        UINT support = 0;
        Hr(enumerator->CheckVideoProcessorFormat(DXGI_FORMAT_NV12, &support), "check NV12 conversion");
        if (!(support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT)) throw std::runtime_error("GPU cannot convert BGRA to NV12");
        Hr(videoDevice->CreateVideoProcessor(enumerator.Get(), 0, &processor), "create GPU converter");
        videoContext->VideoProcessorSetStreamFrameFormat(processor.Get(), 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
        videoContext->VideoProcessorSetStreamAutoProcessingMode(processor.Get(), 0, FALSE);
        D3D11_VIDEO_PROCESSOR_COLOR_SPACE inputColor{}, outputColor{};
        inputColor.Usage = outputColor.Usage = 1;
        inputColor.YCbCr_Matrix = outputColor.YCbCr_Matrix = 1;
        inputColor.Nominal_Range = D3D11_VIDEO_PROCESSOR_NOMINAL_RANGE_0_255;
        outputColor.Nominal_Range = D3D11_VIDEO_PROCESSOR_NOMINAL_RANGE_16_235;
        videoContext->VideoProcessorSetStreamColorSpace(processor.Get(), 0, &inputColor);
        videoContext->VideoProcessorSetOutputColorSpace(processor.Get(), &outputColor);
        hardware.reset(av_hwdevice_ctx_alloc(AV_HWDEVICE_TYPE_D3D11VA));
        if (!hardware) throw std::bad_alloc();
        auto* hw = reinterpret_cast<AVHWDeviceContext*>(hardware->data);
        auto* d3d = static_cast<AVD3D11VADeviceContext*>(hw->hwctx);
        d3d->device = device.Get(); d3d->device->AddRef();
        Check(av_hwdevice_ctx_init(hardware.get()), "initialize FFmpeg D3D11 context");
        frames.reset(av_hwframe_ctx_alloc(hardware.get()));
        if (!frames) throw std::bad_alloc();
        auto* pool = reinterpret_cast<AVHWFramesContext*>(frames->data);
        pool->format = AV_PIX_FMT_D3D11; pool->sw_format = AV_PIX_FMT_NV12;
        pool->width = FrameExchange::Width; pool->height = FrameExchange::Height;
        pool->initial_pool_size = 0; // separate textures, retained by AVFrame until encode completes
        static_cast<AVD3D11VAFramesContext*>(pool->hwctx)->BindFlags = D3D11_BIND_RENDER_TARGET;
        Check(av_hwframe_ctx_init(frames.get()), "initialize NV12 surface pool");
        if (strcmp(requested, "auto") != 0) {
            OpenCodec(requested);
        } else {
            ComPtr<IDXGIDevice> dxgi;
            ComPtr<IDXGIAdapter> adapter;
            DXGI_ADAPTER_DESC desc{};
            Hr(device.As(&dxgi), "query encoder adapter");
            Hr(dxgi->GetAdapter(&adapter), "find encoder adapter");
            Hr(adapter->GetDesc(&desc), "describe encoder adapter");
            const char* hardwareName = desc.VendorId == 0x10de ? "h264_nvenc" : "h264_amf";
            try { OpenCodec(hardwareName); }
            catch (const std::exception& e) {
                fprintf(stderr, "native_encoder fallback=libx264 reason=%s\n", e.what());
                OpenCodec("libx264");
            }
        }
        fprintf(stderr, "native_encoder codec=%s input=%s conversion=d3d11_nv12 cpu_readback=%s ffmpeg=%s adapter_luid=%ld:%lu\n",
            codec->codec->name, synthetic ? "synthetic_d3d11" : "shared_d3d11", software ? "true" : "false", av_version_info(), luid.HighPart, luid.LowPart);
    }

    void OpenCodec(const char* name) {
        if (strcmp(name, "libx264") && strcmp(name, "h264_amf") && strcmp(name, "h264_nvenc"))
            throw std::runtime_error("native encoder must be auto, libx264, h264_amf or h264_nvenc");
        const auto* encoder = avcodec_find_encoder_by_name(name);
        if (!encoder) throw std::runtime_error(std::string("encoder unavailable: ") + name);
        codec.reset(avcodec_alloc_context3(encoder));
        if (!codec) throw std::bad_alloc();
        software = strcmp(name, "libx264") == 0;
        codec->width = FrameExchange::Width; codec->height = FrameExchange::Height;
        codec->time_base = {1, 60}; codec->framerate = {60, 1};
        codec->bit_rate = 16000000; codec->gop_size = 60; codec->max_b_frames = 0;
        codec->thread_count = 1;
        codec->pix_fmt = software ? AV_PIX_FMT_YUV420P : AV_PIX_FMT_D3D11;
        codec->color_range = AVCOL_RANGE_MPEG;
        codec->colorspace = AVCOL_SPC_BT709;
        codec->color_primaries = AVCOL_PRI_BT709;
        codec->color_trc = AVCOL_TRC_BT709;
        codec->flags |= AV_CODEC_FLAG_LOW_DELAY;
        if (!software) {
            codec->hw_frames_ctx = av_buffer_ref(frames.get());
            if (!codec->hw_frames_ctx) throw std::bad_alloc();
        }
        if (software) {
            Check(av_opt_set(codec->priv_data, "preset", "ultrafast", 0), "x264 preset");
            Check(av_opt_set(codec->priv_data, "tune", "zerolatency", 0), "x264 latency");
            Check(av_opt_set(codec->priv_data, "x264-params", "repeat-headers=1:annexb=1:scenecut=0:sync-lookahead=0:rc-lookahead=0:ref=1", 0), "x264 headers");
        } else if (strcmp(name, "h264_nvenc") == 0) {
            Check(av_opt_set(codec->priv_data, "preset", "p1", 0), "NVENC preset");
            Check(av_opt_set(codec->priv_data, "tune", "ull", 0), "NVENC latency");
            Check(av_opt_set_int(codec->priv_data, "zerolatency", 1, 0), "NVENC zero latency");
            Check(av_opt_set_int(codec->priv_data, "delay", 0, 0), "NVENC delay");
        } else {
            Check(av_opt_set(codec->priv_data, "usage", "ultralowlatency", 0), "AMF latency");
            Check(av_opt_set(codec->priv_data, "quality", "speed", 0), "AMF quality");
        }
        Check(avcodec_open2(codec.get(), encoder, nullptr), "open native encoder");
    }

    void Capture() {
        if (synthetic) {
            ComPtr<ID3D11RenderTargetView> view;
            Hr(device->CreateRenderTargetView(latest.Get(), nullptr, &view), "create synthetic pattern");
            const float color[] = { static_cast<float>(number % 3 == 0), static_cast<float>(number % 3 == 1), static_cast<float>(number % 3 == 2), 1.0f };
            context->ClearRenderTargetView(view.Get(), color);
            const auto now = FrameClock();
            latestPacket = {FrameExchange::Magic, FrameExchange::Width, FrameExchange::Height, FrameExchange::Bytes,
                ++lastSequence, 0, now, now, frequency, 0};
            ++fresh;
            return;
        }
        auto* memory = mapping.memory;
        if (InterlockedCompareExchange(&memory->error, 0, 0)) throw std::runtime_error("driver reported a frame exchange error");
        if (!InterlockedCompareExchange(&memory->active, 0, 0) && (lastSequence || Micros() - started > 8000000))
            throw std::runtime_error("virtual display no longer produces surfaces");
        // Claim metadata before reading it. The driver never writes a Reading slot.
        bool claimed[FrameExchange::SlotCount]{};
        int newest = -1;
        for (UINT i = 0; i < FrameExchange::SlotCount; ++i) {
            auto& slot = memory->slots[i];
            if (InterlockedCompareExchange(&slot.state, FrameExchange::Reading, FrameExchange::Ready) != FrameExchange::Ready) continue;
            claimed[i] = true;
            if (newest < 0 || slot.packet.sequence > memory->slots[newest].packet.sequence) newest = static_cast<int>(i);
        }
        for (UINT i = 0; i < FrameExchange::SlotCount; ++i) {
            if (!claimed[i]) continue;
            auto& slot = memory->slots[i];
            const auto metadata = slot.packet;
            const HRESULT lock = shared.locks[i]->AcquireSync(1, 0);
            if (lock == WAIT_TIMEOUT) { InterlockedExchange(&slot.state, FrameExchange::Ready); continue; }
            if (lock != S_OK) {
                InterlockedExchange(&slot.state, FrameExchange::Free);
                throw std::runtime_error("shared GPU keyed mutex lost");
            }
            const auto now = FrameClock();
            const bool valid = static_cast<int>(i) == newest && metadata.sequence > lastSequence &&
                metadata.magic == FrameExchange::Magic && metadata.width == FrameExchange::Width &&
                metadata.height == FrameExchange::Height && metadata.bytes == FrameExchange::Bytes &&
                metadata.frequency == frequency && metadata.acquiredQpc <= metadata.publishedQpc &&
                metadata.publishedQpc <= now && now - metadata.acquiredQpc <= frequency / 20;
            if (valid) {
                context->CopyResource(latest.Get(), shared.textures[i].Get());
                context->Flush();
                latestPacket = metadata; lastSequence = metadata.sequence; ++fresh;
                capturePublishUs += (metadata.publishedQpc - metadata.acquiredQpc) * 1000000 / frequency;
            } else ++stale;
            const auto released = shared.locks[i]->ReleaseSync(0);
            InterlockedExchange(&slot.state, FrameExchange::Free);
            Hr(released, "release shared GPU surface");
        }
    }

    FramePtr Convert() {
        auto output = NewFrame();
        Check(av_hwframe_get_buffer(frames.get(), output.get(), 0), "allocate encoder surface");
        D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC in{};
        in.ViewDimension = D3D11_VPIV_DIMENSION_TEXTURE2D;
        ComPtr<ID3D11VideoProcessorInputView> inputView;
        Hr(videoDevice->CreateVideoProcessorInputView(latest.Get(), enumerator.Get(), &in, &inputView), "create converter input view");
        auto* texture = reinterpret_cast<ID3D11Texture2D*>(output->data[0]);
        D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC out{};
        out.ViewDimension = D3D11_VPOV_DIMENSION_TEXTURE2D;
        ComPtr<ID3D11VideoProcessorOutputView> outputView;
        Hr(videoDevice->CreateVideoProcessorOutputView(texture, enumerator.Get(), &out, &outputView), "create converter output view");
        D3D11_VIDEO_PROCESSOR_STREAM stream{};
        stream.Enable = TRUE; stream.pInputSurface = inputView.Get();
        const auto conversionStarted = Micros();
        Hr(videoContext->VideoProcessorBlt(processor.Get(), outputView.Get(), 0, 1, &stream), "convert BGRA to NV12 on GPU");
        // Event completion includes the GPU work, not just CPU command submission.
        D3D11_QUERY_DESC queryDesc{D3D11_QUERY_EVENT, 0};
        ComPtr<ID3D11Query> completed;
        Hr(device->CreateQuery(&queryDesc, &completed), "create conversion fence");
        context->End(completed.Get()); context->Flush();
        HRESULT ready;
        while ((ready = context->GetData(completed.Get(), nullptr, 0, D3D11_ASYNC_GETDATA_DONOTFLUSH)) == S_FALSE) {
            if (Micros() - conversionStarted > 250000) throw std::runtime_error("GPU conversion expired");
            SwitchToThread();
        }
        Hr(ready, "wait for GPU conversion");
        conversionUs += Micros() - conversionStarted; ++converted;
        if (!software) return output;
        auto downloaded = NewFrame();
        Check(av_hwframe_transfer_data(downloaded.get(), output.get(), 0), "CPU encoder fallback readback");
        auto cpu = NewFrame();
        cpu->width = codec->width; cpu->height = codec->height; cpu->format = codec->pix_fmt;
        Check(av_frame_get_buffer(cpu.get(), 32), "allocate CPU fallback frame");
        scaler.reset(sws_getCachedContext(scaler.release(), cpu->width, cpu->height,
            static_cast<AVPixelFormat>(downloaded->format), cpu->width, cpu->height, codec->pix_fmt,
            SWS_FAST_BILINEAR, nullptr, nullptr, nullptr));
        if (!scaler) throw std::runtime_error("create CPU converter");
        Check(sws_scale(scaler.get(), downloaded->data, downloaded->linesize, 0, cpu->height, cpu->data, cpu->linesize), "convert CPU fallback");
        return cpu;
    }

    bool Next(uint64_t& captured) {
        av_packet_unref(packet.get());
        int result = avcodec_receive_packet(codec.get(), packet.get());
        if (result == AVERROR(EAGAIN)) {
            if (!timer.Wait(schedule.Deadline(), frequency)) throw std::runtime_error("frame timer failed");
            Capture();
            if (!lastSequence) return false;
            if (pending.size() >= 4) throw std::runtime_error("encoder queue exceeded four frames");
            if (lastSequence == emittedSequence) ++repeated;
            emittedSequence = lastSequence;
            auto frame = Convert();
            frame->pts = static_cast<int64_t>(number++);
            const auto qpc = latestPacket.acquiredQpc;
            pending[frame->pts] = {(qpc / frequency) * 1000000 + (qpc % frequency) * 1000000 / frequency, Micros()};
            Check(avcodec_send_frame(codec.get(), frame.get()), "submit encoder surface");
            // Never catch up by emitting bursts after an encoder/network stall.
            schedule.Advance(FrameClock());
            result = avcodec_receive_packet(codec.get(), packet.get());
            if (result == AVERROR(EAGAIN)) return false;
        }
        Check(result, "receive encoded access unit");
        const auto metadata = pending.find(packet->pts);
        if (metadata == pending.end()) throw std::runtime_error("encoder lost frame timestamp");
        captured = metadata->second.first;
        encodeUs += Micros() - metadata->second.second;
        pending.erase(metadata); ++encoded;
        const auto now = Micros();
        if (now - report >= 1000000) {
            fprintf(stderr, "native_encoder_metrics new=%llu repeated=%llu dropped_raw=%llu encoded=%llu encode_fps=%.3f capture_publish_mean_us=%llu conversion_mean_us=%llu encode_mean_us=%llu\n",
                fresh, repeated, stale, encoded, encoded * 1000000.0 / (now - report),
                fresh ? capturePublishUs / fresh : 0, converted ? conversionUs / converted : 0, encoded ? encodeUs / encoded : 0);
            fresh = repeated = stale = converted = encoded = conversionUs = encodeUs = capturePublishUs = 0;
            report = now;
        }
        return true;
    }
};
}

struct RawPacket { const uint8_t* bytes; int len, keyframe; uint64_t captured_micros; };
extern "C" void* wd_encoder_open(const char* mapping, const char* codec) {
    try { return new Encoder(mapping, codec); }
    catch (const std::exception& e) { wd_last_error = e.what(); return nullptr; }
}
extern "C" void wd_encoder_close(void* encoder) { delete static_cast<Encoder*>(encoder); }
extern "C" int wd_encoder_next(void* pointer, RawPacket* output) {
    try {
        auto* encoder = static_cast<Encoder*>(pointer);
        uint64_t captured = 0;
        if (!encoder->Next(captured)) return 0;
        *output = {encoder->packet->data, encoder->packet->size, (encoder->packet->flags & AV_PKT_FLAG_KEY) ? 1 : 0, captured};
        return 1;
    } catch (const std::exception& e) { wd_last_error = e.what(); return -1; }
}

extern "C" int wd_gpu_self_test(const char* codec) {
    try {
        Encoder encoder("", codec, true);
        CodecPtr decoder(avcodec_alloc_context3(avcodec_find_decoder(AV_CODEC_ID_H264)));
        if (!decoder) throw std::bad_alloc();
        Check(avcodec_open2(decoder.get(), decoder->codec, nullptr), "open diagnostic decoder");
        auto frame = NewFrame();
        unsigned count = 0;
        const auto started = Micros();
        while (count < 120 && Micros() - started < 15000000) {
            uint64_t captured = 0;
            if (!encoder.Next(captured)) { Sleep(1); continue; }
            if (!captured) throw std::runtime_error("missing actual capture timestamp");
            Check(avcodec_send_packet(decoder.get(), encoder.packet.get()), "diagnostic decode input");
            Check(avcodec_receive_frame(decoder.get(), frame.get()), "diagnostic decode output");
            if (frame->width != 1280 || frame->height != 800) throw std::runtime_error("diagnostic dimensions changed");
            // BT.709 limited-range luma for the generated R/G/B frames.
            const int expected[] = {63, 173, 32};
            if (std::abs(frame->data[0][0] - expected[count % 3]) > 8)
                throw std::runtime_error("GPU conversion/encoder changed the generated pattern");
            av_frame_unref(frame.get()); ++count;
        }
        if (count != 120) throw std::runtime_error("diagnostic GPU encoder stalled");
        fprintf(stderr, "native_gpu_self_test passed=true requested=%s actual=%s frames=%u elapsed_us=%llu content_verified=true\n",
            codec, encoder.codec->codec->name, count, Micros() - started);
        return 0;
    } catch (const std::exception& e) { wd_last_error = e.what(); return -1; }
}
