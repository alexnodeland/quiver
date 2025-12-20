# Mathematical Foundations

The mathematics underlying Quiver's design and DSP algorithms.

## Category Theory

### Quivers

A **quiver** $Q = (V, E, s, t)$ consists of:
- $V$: Set of vertices (objects)
- $E$: Set of edges (arrows/morphisms)
- $s: E \to V$: Source function
- $t: E \to V$: Target function

In Quiver:
- Vertices = Modules
- Edges = Patch cables
- Source/Target = Output/Input ports

### The Free Category

Given a quiver $Q$, the **free category** $\text{Path}(Q)$ has:
- Objects: Same as $Q$'s vertices
- Morphisms: Paths (sequences of composable arrows)
- Composition: Path concatenation

This is what `patch.compile()` computes.

### Arrow Laws

For arrows $f: A \to B$, $g: B \to C$, $h: C \to D$:

**Identity:**
$$\text{id}_B \circ f = f \circ \text{id}_A = f$$

**Associativity:**
$$(h \circ g) \circ f = h \circ (g \circ f)$$

**First/Second:**
$$\text{first}(f) = f \times \text{id}$$
$$\text{second}(f) = \text{id} \times f$$

## Digital Signal Processing

### Sampling Theory

**Nyquist-Shannon Theorem:**
A signal can be perfectly reconstructed if sampled at rate $f_s > 2f_{max}$.

At 44.1 kHz: $f_{max} = 22.05$ kHz

### Z-Transform

The z-transform converts discrete signals to the z-domain:

$$X(z) = \sum_{n=-\infty}^{\infty} x[n] z^{-n}$$

Unit delay: $z^{-1}$ (one sample delay)

### Transfer Functions

**Lowpass filter (1-pole):**
$$H(z) = \frac{1-p}{1-pz^{-1}}$$

Where $p = e^{-2\pi f_c / f_s}$

**State-Variable Filter:**
$$\begin{aligned}
\text{LP} &= \text{LP}_{n-1} + f \cdot \text{BP}_{n-1} \\
\text{HP} &= \text{input} - \text{LP} - q \cdot \text{BP}_{n-1} \\
\text{BP} &= f \cdot \text{HP} + \text{BP}_{n-1}
\end{aligned}$$

## Waveform Mathematics

### Sine Wave

$$x(t) = A \sin(2\pi f t + \phi)$$

### Sawtooth (Band-Limited)

Fourier series:
$$x(t) = \frac{2}{\pi} \sum_{k=1}^{\infty} \frac{(-1)^{k+1}}{k} \sin(2\pi k f t)$$

### Square Wave

$$x(t) = \frac{4}{\pi} \sum_{k=1,3,5,...}^{\infty} \frac{1}{k} \sin(2\pi k f t)$$

Only odd harmonics!

### Triangle Wave

$$x(t) = \frac{8}{\pi^2} \sum_{k=1,3,5,...}^{\infty} \frac{(-1)^{(k-1)/2}}{k^2} \sin(2\pi k f t)$$

## Envelope Mathematics

### Exponential Segments

**Attack (charging capacitor):**
$$v(t) = V_{max} (1 - e^{-t/\tau})$$

**Decay/Release (discharging):**
$$v(t) = V_{start} \cdot e^{-t/\tau}$$

Time constant $\tau$: time to reach $1 - 1/e \approx 63.2\%$

### RC Time Constant

$$\tau = RC$$

For envelope times: $\tau = \text{time} / \ln(1000) \approx \text{time} / 6.9$

## FM Synthesis

### Basic FM Equation

$$y(t) = A \sin(2\pi f_c t + I \sin(2\pi f_m t))$$

- $f_c$: Carrier frequency
- $f_m$: Modulator frequency
- $I$: Modulation index

### Sidebands

FM produces sidebands at:
$$f_c \pm n \cdot f_m \quad (n = 1, 2, 3, ...)$$

Number of significant sidebands ≈ $I + 1$

### Bessel Functions

Amplitude of each sideband given by Bessel functions:
$$A_n = J_n(I)$$

## Filter Response

### Pole-Zero Form

$$H(z) = \frac{\sum_{k=0}^{M} b_k z^{-k}}{\sum_{k=0}^{N} a_k z^{-k}}$$

### Cutoff Frequency

For bilinear transform:
$$\omega_d = \frac{2}{T} \tan\left(\frac{\omega_a T}{2}\right)$$

### Resonance (Q)

$$Q = \frac{f_0}{\Delta f}$$

Where $\Delta f$ is bandwidth at -3dB.

High Q → narrow peak → self-oscillation

## Analog Modeling

### Thermal Noise

$$V_n = \sqrt{4kTRB}$$

- $k$: Boltzmann constant
- $T$: Temperature (K)
- $R$: Resistance
- $B$: Bandwidth

### Saturation Functions

**Tanh (soft):**
$$y = \tanh(x \cdot \text{drive})$$

**Polynomial (3rd order):**
$$y = x - \frac{x^3}{3}$$

**Asymmetric:**
$$y = \tanh(a \cdot x^+) - \tanh(b \cdot x^-)$$

## V/Oct System

### Pitch to Frequency

$$f = f_0 \cdot 2^V$$

$f_0 = 261.63$ Hz (C4) at 0V

### Frequency to Pitch

$$V = \log_2\left(\frac{f}{f_0}\right)$$

### Semitone

$$\Delta V = \frac{1}{12} \text{ V} \approx 83.33 \text{ mV}$$

### Cent

$$\Delta V = \frac{1}{1200} \text{ V} \approx 0.833 \text{ mV}$$

## SIMD Mathematics

### Vectorized Operations

For 4-wide SIMD:
$$[a_1, a_2, a_3, a_4] + [b_1, b_2, b_3, b_4] = [a_1+b_1, a_2+b_2, a_3+b_3, a_4+b_4]$$

Single instruction, multiple data.

### Block Processing

Process $N$ samples per function call:
- Reduces function call overhead by factor of $N$
- Enables vectorization
- Improves cache locality

## References

- Smith, J.O. *Mathematics of the Discrete Fourier Transform*
- Välimäki, V. *Discrete-Time Synthesis of the Sawtooth Waveform*
- Mac Lane, S. *Categories for the Working Mathematician*
- Chowning, J. *The Synthesis of Complex Audio Spectra by Means of FM*
