#include "livekit/discofast.h"

#include <cstring>

#include "api/make_ref_counted.h"
#include "api/media_stream_interface.h"
#include "livekit/media_stream_track.h"
#include "livekit/rtp_receiver.h"
#include "livekit/rtp_sender.h"
#include "webrtc-sys/src/discofast.rs.h"

namespace livekit_ffi {

DiscofastTransformer::DiscofastTransformer(uint64_t context, bool encrypt)
    : context_(context), encrypt_(encrypt) {}

void DiscofastTransformer::Transform(
    std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
  auto data = frame->GetData();
  rust::Vec<uint8_t> out;
  bool keep = discofast_transform(
      context_, encrypt_, frame->GetSsrc(),
      rust::Slice<const uint8_t>(data.data(), data.size()), out);
  if (!keep) {
    return;
  }
  frame->SetData(std::span<const uint8_t>(out.data(), out.size()));
  webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback;
  {
    std::lock_guard<std::mutex> lock(mutex_);
    auto sink = sinks_.find(frame->GetSsrc());
    callback = sink != sinks_.end() ? sink->second : callback_;
  }
  if (callback) {
    callback->OnTransformedFrame(std::move(frame));
  }
}

void DiscofastTransformer::RegisterTransformedFrameCallback(
    webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback) {
  std::lock_guard<std::mutex> lock(mutex_);
  callback_ = callback;
}

void DiscofastTransformer::RegisterTransformedFrameSinkCallback(
    webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
    uint32_t ssrc) {
  std::lock_guard<std::mutex> lock(mutex_);
  sinks_[ssrc] = callback;
}

void DiscofastTransformer::UnregisterTransformedFrameCallback() {
  std::lock_guard<std::mutex> lock(mutex_);
  callback_ = nullptr;
}

void DiscofastTransformer::UnregisterTransformedFrameSinkCallback(
    uint32_t ssrc) {
  std::lock_guard<std::mutex> lock(mutex_);
  sinks_.erase(ssrc);
}

int32_t DiscofastTap::RecordedDataIsAvailable(const void* samples,
                                              size_t frames,
                                              size_t bytes_per_sample,
                                              size_t channels,
                                              uint32_t rate,
                                              uint32_t delay_ms,
                                              int32_t drift,
                                              uint32_t mic_level,
                                              bool key_pressed,
                                              uint32_t& new_mic_level) {
  return RecordedDataIsAvailable(samples, frames, bytes_per_sample, channels,
                                 rate, delay_ms, drift, mic_level, key_pressed,
                                 new_mic_level, std::nullopt);
}

int32_t DiscofastTap::RecordedDataIsAvailable(
    const void* samples,
    size_t frames,
    size_t bytes_per_sample,
    size_t channels,
    uint32_t rate,
    uint32_t delay_ms,
    int32_t drift,
    uint32_t mic_level,
    bool key_pressed,
    uint32_t& new_mic_level,
    std::optional<int64_t> capture_time_ns) {
  size_t count = frames * channels;
  if (bytes_per_sample == 2 * channels && count > 0) {
    buffer_.resize(count);
    std::memcpy(buffer_.data(), samples, count * sizeof(int16_t));
    discofast_recorded(rust::Slice<int16_t>(buffer_.data(), count), channels,
                       rate);
    samples = buffer_.data();
  }
  return inner_->RecordedDataIsAvailable(
      samples, frames, bytes_per_sample, channels, rate, delay_ms, drift,
      mic_level, key_pressed, new_mic_level, capture_time_ns);
}

int32_t DiscofastTap::NeedMorePlayData(size_t frames,
                                       size_t bytes_per_sample,
                                       size_t channels,
                                       uint32_t rate,
                                       void* samples,
                                       size_t& frames_out,
                                       int64_t* elapsed_time_ms,
                                       int64_t* ntp_time_ms) {
  int32_t result =
      inner_->NeedMorePlayData(frames, bytes_per_sample, channels, rate,
                               samples, frames_out, elapsed_time_ms,
                               ntp_time_ms);
  if (bytes_per_sample == 2 * channels && frames_out > 0) {
    discofast_playout(
        rust::Slice<int16_t>(static_cast<int16_t*>(samples),
                             frames_out * channels),
        channels, rate);
  }
  return result;
}

void DiscofastTap::PullRenderData(int bits_per_sample,
                                  int rate,
                                  size_t channels,
                                  size_t frames,
                                  void* samples,
                                  int64_t* elapsed_time_ms,
                                  int64_t* ntp_time_ms) {
  inner_->PullRenderData(bits_per_sample, rate, channels, frames, samples,
                         elapsed_time_ms, ntp_time_ms);
}

void set_sender_transformer(const RtpSender& sender, uint64_t context) {
  sender.rtc_sender()->SetFrameTransformer(
      webrtc::make_ref_counted<DiscofastTransformer>(context, true));
}

void set_receiver_transformer(const RtpReceiver& receiver, uint64_t context) {
  receiver.rtc_receiver()->SetFrameTransformer(
      webrtc::make_ref_counted<DiscofastTransformer>(context, false));
}

void set_audio_track_volume(const MediaStreamTrack& track, double volume) {
  auto* audio = static_cast<webrtc::AudioTrackInterface*>(track.rtc_track().get());
  if (audio != nullptr && audio->GetSource() != nullptr) {
    audio->GetSource()->SetVolume(volume);
  }
}

}  // namespace livekit_ffi
