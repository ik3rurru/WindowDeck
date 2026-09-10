#pragma once
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <memory>
#include <stdexcept>
#include <string>
extern "C" {
#include <libavcodec/avcodec.h>
#include <libavutil/hwcontext.h>
#include <libavutil/opt.h>
#include <libavutil/pixdesc.h>
#include <libswscale/swscale.h>
}
extern thread_local std::string wd_last_error;
inline void Check(int code, const char* operation) {
    if (code >= 0) return;
    char detail[AV_ERROR_MAX_STRING_SIZE];
    av_strerror(code, detail, sizeof(detail));
    throw std::runtime_error(std::string(operation) + ": " + detail);
}
inline uint64_t Micros() {
    return static_cast<uint64_t>(std::chrono::duration_cast<std::chrono::microseconds>(
        std::chrono::steady_clock::now().time_since_epoch()).count());
}
struct FrameDeleter { void operator()(AVFrame* p) const { av_frame_free(&p); } };
using FramePtr = std::unique_ptr<AVFrame, FrameDeleter>;
inline FramePtr NewFrame() {
    FramePtr frame(av_frame_alloc());
    if (!frame) throw std::bad_alloc();
    return frame;
}
struct CodecDeleter { void operator()(AVCodecContext* p) const { avcodec_free_context(&p); } };
using CodecPtr = std::unique_ptr<AVCodecContext, CodecDeleter>;
struct PacketDeleter { void operator()(AVPacket* p) const { av_packet_free(&p); } };
using PacketPtr = std::unique_ptr<AVPacket, PacketDeleter>;
struct BufferDeleter { void operator()(AVBufferRef* p) const { av_buffer_unref(&p); } };
using BufferPtr = std::unique_ptr<AVBufferRef, BufferDeleter>;
struct ScaleDeleter { void operator()(SwsContext* p) const { sws_freeContext(p); } };
using ScalePtr = std::unique_ptr<SwsContext, ScaleDeleter>;
