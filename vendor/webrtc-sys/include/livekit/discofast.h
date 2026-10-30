// Discofast's additions to webrtc-sys, for Discord's voice:
// - a frame transformer that hands each encoded frame to Rust, where DAVE
//   encrypts what is sent and decrypts what is received, as Discord's web
//   client does with encoded insertable streams;
// - the volume of a remote audio track, as Discord's per-user volume;
// - a tap on the platform audio device's capture and playout, where Rust
//   mixes the soundboard into the microphone, measures its level and sets
//   the output volume.

#pragma once

#include <map>
#include <memory>
#include <mutex>

#include "api/frame_transformer_interface.h"
#include "api/scoped_refptr.h"
#include "modules/audio_device/include/audio_device_defines.h"
#include "rust/cxx.h"

namespace livekit_ffi {

class RtpSender;
class RtpReceiver;
class MediaStreamTrack;

class DiscofastTransformer : public webrtc::FrameTransformerInterface {
 public:
  DiscofastTransformer(uint64_t context, bool encrypt);

  void Transform(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame) override;
  void RegisterTransformedFrameCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback) override;
  void RegisterTransformedFrameSinkCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
      uint32_t ssrc) override;
  void UnregisterTransformedFrameCallback() override;
  void UnregisterTransformedFrameSinkCallback(uint32_t ssrc) override;

 private:
  uint64_t context_;
  bool encrypt_;
  std::mutex mutex_;
  webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback_;
  std::map<uint32_t, webrtc::scoped_refptr<webrtc::TransformedFrameCallback>>
      sinks_;
};

// Passes the platform device's sound through Rust on its way to and from
// the voice engine.
class DiscofastTap : public webrtc::AudioTransport {
 public:
  explicit DiscofastTap(webrtc::AudioTransport* inner) : inner_(inner) {}

  int32_t RecordedDataIsAvailable(const void* samples,
                                  size_t frames,
                                  size_t bytes_per_sample,
                                  size_t channels,
                                  uint32_t rate,
                                  uint32_t delay_ms,
                                  int32_t drift,
                                  uint32_t mic_level,
                                  bool key_pressed,
                                  uint32_t& new_mic_level) override;
  int32_t RecordedDataIsAvailable(
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
      std::optional<int64_t> capture_time_ns) override;
  int32_t NeedMorePlayData(size_t frames,
                           size_t bytes_per_sample,
                           size_t channels,
                           uint32_t rate,
                           void* samples,
                           size_t& frames_out,
                           int64_t* elapsed_time_ms,
                           int64_t* ntp_time_ms) override;
  void PullRenderData(int bits_per_sample,
                      int rate,
                      size_t channels,
                      size_t frames,
                      void* samples,
                      int64_t* elapsed_time_ms,
                      int64_t* ntp_time_ms) override;

 private:
  webrtc::AudioTransport* inner_;
  std::vector<int16_t> buffer_;
};

void set_sender_transformer(const RtpSender& sender, uint64_t context);
void set_receiver_transformer(const RtpReceiver& receiver, uint64_t context);
void set_audio_track_volume(const MediaStreamTrack& track, double volume);

}  // namespace livekit_ffi
