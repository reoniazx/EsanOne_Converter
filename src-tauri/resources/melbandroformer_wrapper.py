"""App-owned MelBandRoFormer vocal-separation CLI.

    python melbandroformer_wrapper.py --weights W --input IN --output-dir OUT \
        --device {cpu|cuda} [--chunk-seconds N]

Writes {output-dir}/melbandroformer_vocals/{input_stem}/vocals.wav and
instrumental.wav.

The model architecture below (RMSNorm/FeedForward/Attention/Transformer/
BandSplit/MaskEstimator/MelBandRoformer) and the chunked overlap-add inference
loop (`_demix`) are a trimmed, inference-only port of
ZFTurbo/Music-Source-Separation-Training (MIT License, Copyright (c) 2024
Roman Solovyev), specifically models/bs_roformer/mel_band_roformer.py,
models/bs_roformer/attend.py, and utils/model_utils.py's demix(). Module
attribute names and ordering are preserved exactly so state_dict keys line up
with the published checkpoint; only training-only code (loss computation,
gradient checkpointing, PoPE, linear-attention blocks, beartype runtime
checks) is removed since this wrapper only ever runs one fixed inference
configuration.

Hyperparameters below are pinned to
configs/KimberleyJensen/config_vocals_mel_band_roformer_kj.yaml from the same
repo, which documents the config used to produce the checkpoint served at
Kijai/MelBandRoFormer_comfy (a safetensors re-export of KimberleyJSN/melbandroformer).
"""

import argparse
import os
import sys

import librosa
import numpy as np
import soundfile as sf
import torch
import torch.nn.functional as F
from einops import rearrange, pack, unpack, reduce, repeat
from librosa import filters as librosa_filters
from rotary_embedding_torch import RotaryEmbedding
from safetensors.torch import load_file as load_safetensors
from torch import nn
from torch.nn import Module, ModuleList

# --- Pinned model/audio config (config_vocals_mel_band_roformer_kj.yaml) ---

SAMPLE_RATE = 44100
NUM_CHANNELS = 2
DEFAULT_CHUNK_SIZE = 352800  # 8s at 44.1kHz; chosen so chunk_size % hop_length == 0
NUM_OVERLAP = 2

MODEL_DIM = 384
MODEL_DEPTH = 6
MODEL_NUM_BANDS = 60
MODEL_DIM_HEAD = 64
MODEL_HEADS = 8
MODEL_TIME_TRANSFORMER_DEPTH = 1
MODEL_FREQ_TRANSFORMER_DEPTH = 1
MODEL_MASK_ESTIMATOR_DEPTH = 2
STFT_N_FFT = 2048
STFT_HOP_LENGTH = 441
STFT_WIN_LENGTH = 2048
STFT_NORMALIZED = False


def _exists(v):
    return v is not None


def _pack_one(t, pattern):
    return pack([t], pattern)


def _unpack_one(t, ps, pattern):
    return unpack(t, ps, pattern)[0]


def _l2norm(t):
    return F.normalize(t, dim=-1, p=2)


class RMSNorm(Module):
    def __init__(self, dim):
        super().__init__()
        self.scale = dim ** 0.5
        self.gamma = nn.Parameter(torch.ones(dim))

    def forward(self, x):
        return F.normalize(x, dim=-1) * self.scale * self.gamma


class FeedForward(Module):
    def __init__(self, dim, mult=4, dropout=0.0):
        super().__init__()
        dim_inner = int(dim * mult)
        self.net = nn.Sequential(
            RMSNorm(dim),
            nn.Linear(dim, dim_inner),
            nn.GELU(),
            nn.Dropout(dropout),
            nn.Linear(dim_inner, dim),
            nn.Dropout(dropout),
        )

    def forward(self, x):
        return self.net(x)


class Attention(Module):
    """Rotary self-attention with a sigmoid output gate (no PoPE, no flash-kernel
    backend pinning — scaled_dot_product_attention picks its own best backend;
    that choice affects speed/precision only, never which parameters exist)."""

    def __init__(self, dim, heads=8, dim_head=64, dropout=0.0, rotary_embed=None):
        super().__init__()
        self.heads = heads
        self.scale = dim_head ** -0.5
        dim_inner = heads * dim_head

        self.rotary_embed = rotary_embed
        self.norm = RMSNorm(dim)
        self.to_qkv = nn.Linear(dim, dim_inner * 3, bias=False)
        self.to_gates = nn.Linear(dim, heads)
        self.to_out = nn.Sequential(
            nn.Linear(dim_inner, dim, bias=False),
            nn.Dropout(dropout),
        )

    def forward(self, x):
        x = self.norm(x)
        q, k, v = rearrange(
            self.to_qkv(x), "b n (qkv h d) -> qkv b h n d", qkv=3, h=self.heads
        )

        if _exists(self.rotary_embed):
            q = self.rotary_embed.rotate_queries_or_keys(q)
            k = self.rotary_embed.rotate_queries_or_keys(k)

        out = F.scaled_dot_product_attention(q, k, v)

        gates = self.to_gates(x)
        out = out * rearrange(gates, "b n h -> b h n 1").sigmoid()

        out = rearrange(out, "b h n d -> b n (h d)")
        return self.to_out(out)


class Transformer(Module):
    def __init__(self, *, dim, depth, dim_head=64, heads=8, attn_dropout=0.0,
                 ff_dropout=0.0, ff_mult=4, rotary_embed=None):
        super().__init__()
        self.layers = ModuleList([])
        for _ in range(depth):
            attn = Attention(
                dim=dim, dim_head=dim_head, heads=heads,
                dropout=attn_dropout, rotary_embed=rotary_embed,
            )
            self.layers.append(
                ModuleList([attn, FeedForward(dim=dim, mult=ff_mult, dropout=ff_dropout)])
            )
        self.norm = RMSNorm(dim)

    def forward(self, x):
        for attn, ff in self.layers:
            x = attn(x) + x
            x = ff(x) + x
        return self.norm(x)


class BandSplit(Module):
    def __init__(self, dim, dim_inputs):
        super().__init__()
        self.dim_inputs = dim_inputs
        self.to_features = ModuleList([])
        for dim_in in dim_inputs:
            self.to_features.append(
                nn.Sequential(RMSNorm(dim_in), nn.Linear(dim_in, dim))
            )

    def forward(self, x):
        x = x.split(self.dim_inputs, dim=-1)
        outs = [to_feature(split_input) for split_input, to_feature in zip(x, self.to_features)]
        return torch.stack(outs, dim=-2)


def _mlp(dim_in, dim_out, dim_hidden, depth, activation=nn.Tanh):
    net = []
    dims = (dim_in, *((dim_hidden,) * depth), dim_out)
    for ind, (layer_dim_in, layer_dim_out) in enumerate(zip(dims[:-1], dims[1:])):
        net.append(nn.Linear(layer_dim_in, layer_dim_out))
        if ind != len(dims) - 2:
            net.append(activation())
    return nn.Sequential(*net)


class MaskEstimator(Module):
    def __init__(self, dim, dim_inputs, depth, mlp_expansion_factor=4):
        super().__init__()
        self.dim_inputs = dim_inputs
        self.to_freqs = ModuleList([])
        dim_hidden = dim * mlp_expansion_factor
        for dim_in in dim_inputs:
            self.to_freqs.append(
                nn.Sequential(
                    _mlp(dim, dim_in * 2, dim_hidden=dim_hidden, depth=depth),
                    nn.GLU(dim=-1),
                )
            )

    def forward(self, x):
        x = x.unbind(dim=-2)
        outs = [mlp(band_features) for band_features, mlp in zip(x, self.to_freqs)]
        return torch.cat(outs, dim=-1)


class MelBandRoformer(Module):
    """Single-stem (vocals) inference-only Mel-Band RoFormer. See module
    docstring: ported from ZFTurbo/Music-Source-Separation-Training (MIT)."""

    def __init__(self, dim, *, depth, stereo, num_bands, dim_head, heads,
                 time_transformer_depth, freq_transformer_depth,
                 mask_estimator_depth, sample_rate, stft_n_fft, stft_hop_length,
                 stft_win_length, stft_normalized, attn_dropout=0.0, ff_dropout=0.0,
                 mlp_expansion_factor=4, zero_dc=True):
        super().__init__()

        self.stereo = stereo
        self.audio_channels = 2 if stereo else 1
        self.zero_dc = zero_dc

        self.layers = ModuleList([])
        time_rotary_embed = RotaryEmbedding(dim=dim_head)
        freq_rotary_embed = RotaryEmbedding(dim=dim_head)

        transformer_kwargs = dict(
            dim=dim, heads=heads, dim_head=dim_head,
            attn_dropout=attn_dropout, ff_dropout=ff_dropout,
        )
        for _ in range(depth):
            self.layers.append(
                ModuleList([
                    Transformer(depth=time_transformer_depth, rotary_embed=time_rotary_embed, **transformer_kwargs),
                    Transformer(depth=freq_transformer_depth, rotary_embed=freq_rotary_embed, **transformer_kwargs),
                ])
            )

        self.stft_window_fn = lambda device=None: torch.hann_window(stft_win_length, device=device)
        self.stft_kwargs = dict(
            n_fft=stft_n_fft, hop_length=stft_hop_length,
            win_length=stft_win_length, normalized=stft_normalized,
        )

        freqs = torch.stft(
            torch.randn(1, 4096), **self.stft_kwargs,
            window=torch.ones(stft_n_fft), return_complex=True,
        ).shape[1]

        mel_filter_bank = torch.from_numpy(
            librosa_filters.mel(sr=sample_rate, n_fft=stft_n_fft, n_mels=num_bands)
        )
        # Matches upstream exactly: band 0 doesn't natively cover freq 0, and
        # some environments return 0.0 instead of ~1.9e-18 at the last cell.
        mel_filter_bank[0][0] = 1.0
        mel_filter_bank[-1, -1] = 1.0

        freqs_per_band = mel_filter_bank > 0
        assert freqs_per_band.any(dim=0).all(), "all frequencies need to be covered by all bands"

        repeated_freq_indices = repeat(torch.arange(freqs), "f -> b f", b=num_bands)
        freq_indices = repeated_freq_indices[freqs_per_band]

        if stereo:
            freq_indices = repeat(freq_indices, "f -> f s", s=2)
            freq_indices = freq_indices * 2 + torch.arange(2)
            freq_indices = rearrange(freq_indices, "f s -> (f s)")

        self.register_buffer("freq_indices", freq_indices, persistent=False)
        self.register_buffer("freqs_per_band", freqs_per_band, persistent=False)

        num_freqs_per_band = reduce(freqs_per_band, "b f -> b", "sum")
        num_bands_per_freq = reduce(freqs_per_band, "b f -> f", "sum")
        self.register_buffer("num_freqs_per_band", num_freqs_per_band, persistent=False)
        self.register_buffer("num_bands_per_freq", num_bands_per_freq, persistent=False)

        freqs_per_bands_with_complex = tuple(
            2 * f * self.audio_channels for f in num_freqs_per_band.tolist()
        )

        self.band_split = BandSplit(dim=dim, dim_inputs=freqs_per_bands_with_complex)
        self.mask_estimators = nn.ModuleList([
            MaskEstimator(
                dim=dim, dim_inputs=freqs_per_bands_with_complex,
                depth=mask_estimator_depth, mlp_expansion_factor=mlp_expansion_factor,
            )
        ])

    def forward(self, raw_audio):
        device = raw_audio.device
        if raw_audio.ndim == 2:
            raw_audio = rearrange(raw_audio, "b t -> b 1 t")
        batch, channels, _ = raw_audio.shape
        assert channels == (2 if self.stereo else 1)

        raw_audio, packed_shape = _pack_one(raw_audio, "* t")
        stft_window = self.stft_window_fn(device=device)
        stft_repr = torch.stft(raw_audio, **self.stft_kwargs, window=stft_window, return_complex=True)
        stft_repr = torch.view_as_real(stft_repr)
        stft_repr = _unpack_one(stft_repr, packed_shape, "* f t c")
        stft_repr = rearrange(stft_repr, "b s f t c -> b (f s) t c")

        batch_arange = torch.arange(batch, device=device)[..., None]
        x = stft_repr[batch_arange, self.freq_indices]
        x = rearrange(x, "b f t c -> b t (f c)")

        x = self.band_split(x)

        for time_transformer, freq_transformer in self.layers:
            x = rearrange(x, "b t f d -> b f t d")
            x, ps = pack([x], "* t d")
            x = time_transformer(x)
            x, = unpack(x, ps, "* t d")
            x = rearrange(x, "b f t d -> b t f d")
            x, ps = pack([x], "* f d")
            x = freq_transformer(x)
            x, = unpack(x, ps, "* f d")

        mask = self.mask_estimators[0](x)
        mask = rearrange(mask, "b t (f c) -> b f t c", c=2)

        stft_repr = rearrange(stft_repr, "b f t c -> b 1 f t c")
        stft_repr = torch.view_as_complex(stft_repr)
        mask = torch.view_as_complex(mask.unsqueeze(1))
        mask = mask.type(stft_repr.dtype)

        scatter_indices = repeat(self.freq_indices, "f -> b 1 f t", b=batch, t=stft_repr.shape[-1])
        masks_summed = torch.zeros_like(stft_repr).scatter_add_(2, scatter_indices, mask)
        denom = repeat(self.num_bands_per_freq, "f -> (f r) 1", r=channels)
        masks_averaged = masks_summed / denom.clamp(min=1e-8)

        stft_repr = stft_repr * masks_averaged
        stft_repr = rearrange(stft_repr, "b n (f s) t -> (b n s) f t", s=self.audio_channels)

        if self.zero_dc:
            stft_repr = stft_repr.index_fill(1, torch.tensor(0, device=device), 0.0)

        recon_audio = torch.istft(stft_repr, **self.stft_kwargs, window=stft_window, return_complex=False)
        recon_audio = rearrange(recon_audio, "(b n s) t -> b n s t", b=batch, s=self.audio_channels, n=1)
        return rearrange(recon_audio, "b 1 s t -> b s t")


def _build_model() -> MelBandRoformer:
    return MelBandRoformer(
        dim=MODEL_DIM,
        depth=MODEL_DEPTH,
        stereo=True,
        num_bands=MODEL_NUM_BANDS,
        dim_head=MODEL_DIM_HEAD,
        heads=MODEL_HEADS,
        time_transformer_depth=MODEL_TIME_TRANSFORMER_DEPTH,
        freq_transformer_depth=MODEL_FREQ_TRANSFORMER_DEPTH,
        mask_estimator_depth=MODEL_MASK_ESTIMATOR_DEPTH,
        sample_rate=SAMPLE_RATE,
        stft_n_fft=STFT_N_FFT,
        stft_hop_length=STFT_HOP_LENGTH,
        stft_win_length=STFT_WIN_LENGTH,
        stft_normalized=STFT_NORMALIZED,
    )


def _windowing_array(window_size: int, fade_size: int) -> torch.Tensor:
    """Linear fade-in/fade-out window used to overlap-add adjacent chunks
    (ported from utils/model_utils.py::_getWindowingArray)."""
    window = torch.ones(window_size)
    window[-fade_size:] = torch.linspace(1, 0, fade_size)
    window[:fade_size] = torch.linspace(0, 1, fade_size)
    return window


def _print_progress(done: int, total: int) -> None:
    percent = min(100, int(done * 100 / max(total, 1)))
    bar_len = 24
    filled = int(bar_len * percent / 100)
    bar = "#" * filled + " " * (bar_len - filled)
    # Deliberately plain integer counts (no size units) so the frontend's
    # tqdm-style parser reads this as separation progress, not a download.
    print(f"{percent:3d}%|{bar}| {done}/{total}", flush=True)


def _demix_vocals(model: MelBandRoformer, mix: torch.Tensor, device: str, chunk_size: int) -> np.ndarray:
    """Chunked overlap-add inference for one 'generic' single-stem model.
    Ported from utils/model_utils.py::demix (mode="generic", num_instruments=1,
    bigshifts=1). mix: (channels, samples) float32."""
    num_overlap = NUM_OVERLAP
    fade_size = max(1, chunk_size // 10)
    step = chunk_size // num_overlap
    border = chunk_size - step
    length_init = mix.shape[-1]
    windowing_array = _windowing_array(chunk_size, fade_size)

    if length_init > 2 * border and border > 0:
        mix = F.pad(mix, (border, border), mode="reflect")

    result = torch.zeros((1,) + mix.shape, dtype=torch.float32)
    counter = torch.zeros((1,) + mix.shape, dtype=torch.float32)

    total = mix.shape[1]
    i = 0
    with torch.inference_mode():
        while i < mix.shape[1]:
            part = mix[:, i:i + chunk_size].to(device)
            chunk_len = part.shape[-1]
            pad_mode = "reflect" if chunk_len > chunk_size // 2 else "constant"
            part = F.pad(part, (0, chunk_size - chunk_len), mode=pad_mode, value=0)

            out = model(part.unsqueeze(0))[0].to(torch.float32).cpu()

            window = windowing_array.clone()
            if i == 0:
                window[:fade_size] = 1
            if i + step >= mix.shape[1]:
                window[-fade_size:] = 1

            result[0, :, i:i + chunk_len] += out[..., :chunk_len] * window[..., :chunk_len]
            counter[0, :, i:i + chunk_len] += window[..., :chunk_len]

            i += step
            _print_progress(min(i, total), total)

    estimated = (result / counter).numpy()
    np.nan_to_num(estimated, copy=False, nan=0.0)
    if length_init > 2 * border and border > 0:
        estimated = estimated[..., border:-border]
    return estimated[0]


def main() -> None:
    parser = argparse.ArgumentParser(description="MelBandRoFormer vocal separation")
    parser.add_argument("--weights", required=True)
    parser.add_argument("--input", required=True)
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--device", choices=["cpu", "cuda"], default="cpu")
    parser.add_argument("--chunk-seconds", type=float, default=None)
    args = parser.parse_args()

    device = args.device if (args.device == "cpu" or torch.cuda.is_available()) else "cpu"

    chunk_size = DEFAULT_CHUNK_SIZE
    if args.chunk_seconds:
        # Keep chunk_size a multiple of hop_length so STFT/ISTFT round-trips
        # to the exact chunk length, same as the pinned default.
        requested = int(args.chunk_seconds * SAMPLE_RATE)
        hops = max(4, round(requested / STFT_HOP_LENGTH))
        chunk_size = hops * STFT_HOP_LENGTH

    mix, _ = librosa.load(args.input, sr=SAMPLE_RATE, mono=False)
    if mix.ndim == 1:
        mix = np.stack([mix, mix], axis=0)
    mix_orig = mix.copy()
    mix_t = torch.tensor(mix, dtype=torch.float32)

    model = _build_model()
    state_dict = load_safetensors(args.weights)
    model.load_state_dict(state_dict, strict=True)
    model.eval()
    model.to(device)

    vocals = _demix_vocals(model, mix_t, device, chunk_size)
    common_len = min(vocals.shape[-1], mix_orig.shape[-1])
    vocals = vocals[..., :common_len]
    instrumental = mix_orig[..., :common_len] - vocals

    stem_name = os.path.splitext(os.path.basename(args.input))[0]
    out_dir = os.path.join(args.output_dir, "melbandroformer_vocals", stem_name)
    os.makedirs(out_dir, exist_ok=True)
    sf.write(os.path.join(out_dir, "vocals.wav"), vocals.T, SAMPLE_RATE, subtype="FLOAT")
    sf.write(os.path.join(out_dir, "instrumental.wav"), instrumental.T, SAMPLE_RATE, subtype="FLOAT")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001 - re-raised below; message must keep OOM/error text intact
        print(f"melbandroformer_wrapper error: {exc}", file=sys.stderr, flush=True)
        raise
