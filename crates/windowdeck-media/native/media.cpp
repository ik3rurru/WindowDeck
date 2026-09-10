#include "common.h"
#define SDL_MAIN_HANDLED
#include <SDL.h>
#include <algorithm>
#include <cstring>
#include <map>

thread_local std::string wd_last_error;
extern "C" const char* wd_error() { return wd_last_error.c_str(); }
extern "C" const char* wd_version() { return av_version_info(); }

namespace {
struct WindowDeleter { void operator()(SDL_Window* p) const { SDL_DestroyWindow(p); } };
struct RendererDeleter { void operator()(SDL_Renderer* p) const { SDL_DestroyRenderer(p); } };
struct TextureDeleter { void operator()(SDL_Texture* p) const { SDL_DestroyTexture(p); } };
void SdlCheck(int result) { if (result < 0) throw std::runtime_error(SDL_GetError()); }
struct Player {
    CodecPtr decoder;
    BufferPtr hardware;
    AVPixelFormat hardwareFormat = AV_PIX_FMT_NONE;
    std::unique_ptr<SDL_Window, WindowDeleter> window;
    std::unique_ptr<SDL_Renderer, RendererDeleter> renderer;
    std::unique_ptr<SDL_Texture, TextureDeleter> texture;
    FramePtr latest;
    ScalePtr scaler;
    int width = 0, height = 0;
    bool fullscreen = false, ready = false;
    uint64_t decoded = 0, presented = 0, dropped = 0, decodeUs = 0, uploadUs = 0, presentUs = 0;
    uint64_t latestReceived = 0, report = Micros(), rxToPresentUs = 0, totalPresented = 0;
    std::map<int64_t, uint64_t> received;

    static AVPixelFormat Format(AVCodecContext* context, const AVPixelFormat* formats) {
        auto* self = static_cast<Player*>(context->opaque);
        for (auto* format = formats; *format != AV_PIX_FMT_NONE; ++format) {
            if (*format == self->hardwareFormat) {
                fprintf(stderr, "native_decoder active=%s hardware=true\n", av_get_pix_fmt_name(*format));
                return *format;
            }
        }
        for (auto* format = formats; *format != AV_PIX_FMT_NONE; ++format) {
            const auto* descriptor = av_pix_fmt_desc_get(*format);
            if (descriptor && !(descriptor->flags & AV_PIX_FMT_FLAG_HWACCEL)) {
                fprintf(stderr, "native_decoder active=%s hardware=false\n", av_get_pix_fmt_name(*format));
                return *format;
            }
        }
        return AV_PIX_FMT_NONE;
    }

    void OpenDecoder() {
        const auto* codec = avcodec_find_decoder(AV_CODEC_ID_H264);
        if (!codec) throw std::runtime_error("H.264 decoder unavailable");
        decoder.reset(avcodec_alloc_context3(codec));
        if (!decoder) throw std::bad_alloc();
        decoder->flags |= AV_CODEC_FLAG_LOW_DELAY;
        decoder->thread_count = 1;
        decoder->opaque = this;
        decoder->get_format = Format;
        decoder->max_pixels = 1280 * 800;
        if (hardware) decoder->hw_device_ctx = av_buffer_ref(hardware.get());
        Check(avcodec_open2(decoder.get(), codec, nullptr), "open H.264 decoder");
    }

    explicit Player(bool full) : fullscreen(full) {
        SDL_SetMainReady();
        SdlCheck(SDL_Init(SDL_INIT_VIDEO | SDL_INIT_EVENTS));
        window.reset(SDL_CreateWindow("WindowDeck", SDL_WINDOWPOS_CENTERED, SDL_WINDOWPOS_CENTERED,
            1280, 800, SDL_WINDOW_RESIZABLE | (full ? SDL_WINDOW_FULLSCREEN_DESKTOP : 0)));
        if (!window) throw std::runtime_error(SDL_GetError());
        renderer.reset(SDL_CreateRenderer(window.get(), -1, SDL_RENDERER_ACCELERATED));
        if (!renderer) renderer.reset(SDL_CreateRenderer(window.get(), -1, SDL_RENDERER_SOFTWARE));
        if (!renderer) throw std::runtime_error(SDL_GetError());
        SDL_RendererInfo info{};
        SdlCheck(SDL_GetRendererInfo(renderer.get(), &info));
        fprintf(stderr, "native_renderer name=%s accelerated=%s ffmpeg=%s\n", info.name,
            (info.flags & SDL_RENDERER_ACCELERATED) ? "true" : "false", av_version_info());
#ifdef _WIN32
        const auto type = AV_HWDEVICE_TYPE_D3D11VA;
        hardwareFormat = AV_PIX_FMT_D3D11;
#else
        const auto type = AV_HWDEVICE_TYPE_VAAPI;
        hardwareFormat = AV_PIX_FMT_VAAPI;
#endif
        AVBufferRef* device = nullptr;
        const int result = av_hwdevice_ctx_create(&device, type, nullptr, nullptr, 0);
        hardware.reset(device);
        if (result < 0) {
            hardwareFormat = AV_PIX_FMT_NONE;
            fprintf(stderr, "native_decoder fallback=software reason=hardware_device_unavailable\n");
        }
        OpenDecoder();
    }

    void Receive() {
        auto frame = NewFrame();
        for (;;) {
            const int result = avcodec_receive_frame(decoder.get(), frame.get());
            if (result == AVERROR(EAGAIN) || result == AVERROR_EOF) break;
            Check(result, "decode H.264 frame");
            ++decoded;
            if (ready) ++dropped;
            const auto arrival = received.find(frame->pts);
            latestReceived = arrival == received.end() ? Micros() : arrival->second;
            if (arrival != received.end()) received.erase(received.begin(), std::next(arrival));
            latest = std::move(frame);
            ready = true;
            frame = NewFrame();
        }
    }

    void Packet(const uint8_t* bytes, int len, int64_t pts) {
        if (len <= 0 || len > 4 * 1024 * 1024) throw std::runtime_error("invalid H.264 access unit size");
        PacketPtr packet(av_packet_alloc());
        if (!packet) throw std::bad_alloc();
        Check(av_new_packet(packet.get(), len), "allocate padded input packet");
        memcpy(packet->data, bytes, static_cast<size_t>(len));
        packet->pts = packet->dts = pts;
        const auto started = Micros();
        received[pts] = started;
        if (received.size() > 8) throw std::runtime_error("decoder did not consume eight access units");
        int result = avcodec_send_packet(decoder.get(), packet.get());
        if (result == AVERROR(EAGAIN)) {
            Receive();
            result = avcodec_send_packet(decoder.get(), packet.get());
        }
        Check(result, "submit H.264 packet");
        Receive();
        decodeUs += Micros() - started;
    }

    bool Poll() {
        SDL_Event event;
        while (SDL_PollEvent(&event)) {
            if (event.type == SDL_QUIT) return false;
            if (event.type == SDL_KEYDOWN) {
                if (event.key.keysym.sym == SDLK_F11) fullscreen = !fullscreen;
                else if (event.key.keysym.sym == SDLK_ESCAPE) fullscreen = false;
                else continue;
                SdlCheck(SDL_SetWindowFullscreen(window.get(), fullscreen ? SDL_WINDOW_FULLSCREEN_DESKTOP : 0));
            }
        }
        if (ready) {
            ready = false;
            if (Micros() - latestReceived > 250000) { ++dropped; latest.reset(); return true; }
            const auto uploadStarted = Micros();
            FramePtr transfer;
            AVFrame* frame = latest.get();
            if (frame->format == hardwareFormat && hardwareFormat != AV_PIX_FMT_NONE) {
                transfer = NewFrame();
                Check(av_hwframe_transfer_data(transfer.get(), frame, 0), "download decoded frame for SDL");
                frame = transfer.get();
            }
            if (frame->width <= 0 || frame->height <= 0 || frame->width > 1280 || frame->height > 800)
                throw std::runtime_error("decoded dimensions exceed the negotiated mode");
            if (width != frame->width || height != frame->height) {
                width = frame->width; height = frame->height;
                texture.reset(SDL_CreateTexture(renderer.get(), SDL_PIXELFORMAT_IYUV, SDL_TEXTUREACCESS_STREAMING, width, height));
                if (!texture) throw std::runtime_error(SDL_GetError());
                SdlCheck(SDL_RenderSetLogicalSize(renderer.get(), width, height));
            }
            FramePtr converted;
            if (frame->format != AV_PIX_FMT_YUV420P) {
                converted = NewFrame();
                converted->width = width; converted->height = height; converted->format = AV_PIX_FMT_YUV420P;
                Check(av_frame_get_buffer(converted.get(), 32), "allocate display frame");
                scaler.reset(sws_getCachedContext(scaler.release(), width, height, static_cast<AVPixelFormat>(frame->format),
                    width, height, AV_PIX_FMT_YUV420P, SWS_FAST_BILINEAR, nullptr, nullptr, nullptr));
                if (!scaler) throw std::runtime_error("create display converter");
                Check(sws_scale(scaler.get(), frame->data, frame->linesize, 0, height,
                    converted->data, converted->linesize), "convert display frame");
                frame = converted.get();
            }
            SdlCheck(SDL_UpdateYUVTexture(texture.get(), nullptr, frame->data[0], frame->linesize[0],
                frame->data[1], frame->linesize[1], frame->data[2], frame->linesize[2]));
            uploadUs += Micros() - uploadStarted;
            SdlCheck(SDL_RenderClear(renderer.get()));
            SdlCheck(SDL_RenderCopy(renderer.get(), texture.get(), nullptr, nullptr));
            const auto beforePresent = Micros();
            SDL_RenderPresent(renderer.get());
            const auto now = Micros();
            presentUs += now - beforePresent;
            rxToPresentUs += now - latestReceived;
            ++presented;
            ++totalPresented;
            SDL_SetWindowTitle(window.get(), "WindowDeck");
            latest.reset();
        }
        const auto now = Micros();
        if (now - report >= 1000000) {
            fprintf(stderr, "native_player_metrics decoded=%llu presented=%llu dropped=%llu present_fps=%.3f decode_mean_us=%llu upload_mean_us=%llu present_mean_us=%llu receive_to_present_mean_us=%llu\n",
                static_cast<unsigned long long>(decoded), static_cast<unsigned long long>(presented), static_cast<unsigned long long>(dropped),
                presented * 1000000.0 / (now - report), static_cast<unsigned long long>(decoded ? decodeUs / decoded : 0),
                static_cast<unsigned long long>(presented ? uploadUs / presented : 0), static_cast<unsigned long long>(presented ? presentUs / presented : 0),
                static_cast<unsigned long long>(presented ? rxToPresentUs / presented : 0));
            decoded = presented = dropped = decodeUs = uploadUs = presentUs = rxToPresentUs = 0;
            report = now;
        }
        return true;
    }
    void Reset() {
        latest.reset(); ready = false; received.clear();
        OpenDecoder();
        SDL_SetWindowTitle(window.get(), "WindowDeck - reconectando...");
    }
};

void SelfTest() {
    // Separate diagnostic process: never changes the user's display or captures it.
    SDL_setenv("SDL_VIDEODRIVER", "dummy", 1);
    Player player(false);
    const AVCodec* implementation = avcodec_find_encoder_by_name("libx264");
    if (!implementation) throw std::runtime_error("self-test requires libx264");
    CodecPtr encoder(avcodec_alloc_context3(implementation));
    if (!encoder) throw std::bad_alloc();
    encoder->width = 128; encoder->height = 80;
    encoder->time_base = {1, 60}; encoder->framerate = {60, 1};
    encoder->pix_fmt = AV_PIX_FMT_YUV420P;
    encoder->gop_size = 1; encoder->max_b_frames = 0;
    Check(av_opt_set(encoder->priv_data, "preset", "ultrafast", 0), "test preset");
    Check(av_opt_set(encoder->priv_data, "tune", "zerolatency", 0), "test latency");
    Check(avcodec_open2(encoder.get(), implementation, nullptr), "test encoder");
    PacketPtr packet(av_packet_alloc());
    if (!packet) throw std::bad_alloc();
    for (int cycle = 0; cycle < 3; ++cycle) {
        player.Reset();
        for (int i = 0; i < 6; ++i) {
            auto frame = NewFrame();
            frame->width = 128; frame->height = 80; frame->format = AV_PIX_FMT_YUV420P;
            frame->pts = cycle * 6 + i;
            Check(av_frame_get_buffer(frame.get(), 32), "test frame");
            for (int row = 0; row < 80; ++row) memset(frame->data[0] + row * frame->linesize[0], 40 + i * 20, 128);
            for (int plane = 1; plane < 3; ++plane)
                for (int row = 0; row < 40; ++row) memset(frame->data[plane] + row * frame->linesize[plane], 128, 64);
            Check(avcodec_send_frame(encoder.get(), frame.get()), "test encode");
            Check(avcodec_receive_packet(encoder.get(), packet.get()), "test access unit");
            player.Packet(packet->data, packet->size, frame->pts);
            if (!player.latest) throw std::runtime_error("test decoder produced no image");
            auto pixels = NewFrame();
            const AVFrame* decoded = player.latest.get();
            if (decoded->hw_frames_ctx) { Check(av_hwframe_transfer_data(pixels.get(), decoded, 0), "test readback"); decoded = pixels.get(); }
            if (decoded->width != 128 || decoded->height != 80 || std::abs(decoded->data[0][0] - (40 + i * 20)) > 4)
                throw std::runtime_error("test decoded image does not match the generated pattern");
            if (!player.Poll()) throw std::runtime_error("test player closed");
            av_packet_unref(packet.get());
        }
    }
    if (player.totalPresented != 18) throw std::runtime_error("test did not present all 18 frames");
    fprintf(stderr, "native_self_test passed=true frames=18 decoder_resets=3 content_verified=true\n");
}
}

extern "C" void* wd_player_open(int fullscreen) {
    try { return new Player(fullscreen != 0); }
    catch (const std::exception& e) { wd_last_error = e.what(); SDL_Quit(); return nullptr; }
}
extern "C" void wd_player_close(void* player) { delete static_cast<Player*>(player); SDL_Quit(); }
extern "C" int wd_player_packet(void* player, const uint8_t* bytes, int len, int64_t pts) {
    try { static_cast<Player*>(player)->Packet(bytes, len, pts); return 0; }
    catch (const std::exception& e) { wd_last_error = e.what(); return -1; }
}
extern "C" int wd_player_poll(void* player) {
    try { return static_cast<Player*>(player)->Poll() ? 1 : 0; }
    catch (const std::exception& e) { wd_last_error = e.what(); return -1; }
}
extern "C" int wd_player_reset(void* player) {
    try { static_cast<Player*>(player)->Reset(); return 0; }
    catch (const std::exception& e) { wd_last_error = e.what(); return -1; }
}
extern "C" int wd_self_test() {
    try { SelfTest(); SDL_Quit(); return 0; }
    catch (const std::exception& e) { wd_last_error = e.what(); SDL_Quit(); return -1; }
}
