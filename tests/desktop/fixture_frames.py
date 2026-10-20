"""Read authoritative state separately from protocol-only liveness frames."""


def latest_state_frame(frames):
    for frame in reversed(frames):
        if not isinstance(frame, dict):
            raise ValueError('Expected an object transport frame')
        if frame.get('liveness') is not True:
            return frame
    return {}
