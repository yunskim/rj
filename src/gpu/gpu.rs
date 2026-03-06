/// GPU 추상화 레이어
///
/// 설계 원칙:
///   1. CPU 코드는 GPU를 전혀 몰라도 됨
///   2. GpuDevice가 없으면 GpuBuffer 생성 불가 → 컴파일 타임 보장
///   3. feature = "gpu" 없으면 이 파일의 실제 구현은 컴파일되지 않음
///      → CPU 전용 빌드에서 wgpu 의존성 없음
///
/// 데이터 전송 흐름:
///   CPU Vec<f64>
///     → GpuBuffer::from_cpu(dev, data)    [CPU→GPU, 1번만]
///     → GPU 커널들이 VRAM에서 직접 연산    [전송 없음]
///     → GpuBuffer::to_cpu()               [GPU→CPU, 출력 시에만]

// ─────────────────────────────────────────
// GpuBuffer - VRAM에 있는 데이터
// ─────────────────────────────────────────

/// VRAM에 있는 flat f64 배열
/// J의 JData::Float/Complex 와 대응
/// Arc로 감싸서 JVal 간에 공유 (복사 없이)
#[derive(Clone, Debug)]
pub struct GpuBuffer {
    /// 원소 수 (Float이면 count, Complex이면 쌍의 수)
    pub count:     usize,
    /// Float 또는 Complex (쌍) 구분
    pub elem_type: GpuElemType,

    // ── 실제 구현 ──
    #[cfg(feature = "gpu")]
    pub(crate) inner: GpuBufferInner,

    // ── CPU 폴백 (gpu feature 없을 때) ──
    // GpuBuffer가 생성될 일이 없으므로 사실상 dead code
    // 타입 시스템상 존재해야 컴파일 가능
    #[cfg(not(feature = "gpu"))]
    pub(crate) _phantom: std::marker::PhantomData<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GpuElemType {
    Float,    // f64 하나가 1원소
    Complex,  // f64 두 개 (r, i)가 1원소
}

// ─────────────────────────────────────────
// GPU feature가 있을 때의 실제 구현
// ─────────────────────────────────────────

#[cfg(feature = "gpu")]
pub(crate) struct GpuBufferInner {
    /// wgpu 버퍼 (VRAM)
    pub buffer: wgpu::Buffer,
    /// 바이트 크기
    pub size:   u64,
}

#[cfg(feature = "gpu")]
impl Clone for GpuBufferInner {
    fn clone(&self) -> Self {
        // wgpu Buffer는 Clone이 없음
        // JVal이 Arc<JArray>이므로 실제로 clone이 불릴 일은 없음
        // 혹시 불리면 panic으로 명시
        panic!("GpuBuffer should be shared via Arc, not cloned")
    }
}

#[cfg(feature = "gpu")]
impl std::fmt::Debug for GpuBufferInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GpuBufferInner(size={})", self.size)
    }
}

// ─────────────────────────────────────────
// GpuDevice - GPU 연산 컨텍스트
// ─────────────────────────────────────────

/// GPU 연산에 필요한 컨텍스트
/// Interpreter가 보유, rank1ex_gpu 등에 전달
#[derive(Debug)]
pub struct GpuDevice {
    #[cfg(feature = "gpu")]
    pub(crate) device:  wgpu::Device,
    #[cfg(feature = "gpu")]
    pub(crate) queue:   wgpu::Queue,

    /// 미리 컴파일된 커널 (셰이더)
    #[cfg(feature = "gpu")]
    pub(crate) kernels: GpuKernels,
}

/// 미리 컴파일된 GPU 커널 모음
/// 동사마다 커널이 있음
#[cfg(feature = "gpu")]
pub(crate) struct GpuKernels {
    /// element-wise float 연산: add, sub, mul, div
    pub float_binop:  wgpu::ComputePipeline,
    /// element-wise complex 연산
    pub complex_binop: wgpu::ComputePipeline,
    /// reduction: +/ (sum), */ (product)
    pub reduce_sum:   wgpu::ComputePipeline,
    pub reduce_prod:  wgpu::ComputePipeline,
}

// ─────────────────────────────────────────
// Backend - CPU/GPU 선택
// ─────────────────────────────────────────

/// 연산 백엔드
/// Interpreter에 저장, rank1ex/rank2ex에 전달
#[derive(Debug)]
pub enum Backend {
    /// CPU 연산 (기본값)
    /// rayon으로 병렬화 가능
    Cpu,

    /// GPU 연산
    /// feature = "gpu" 없으면 이 변형을 만들 수 없음
    #[cfg(feature = "gpu")]
    Gpu(GpuDevice),
}

impl Backend {
    pub fn is_gpu(&self) -> bool {
        #[cfg(feature = "gpu")]
        if let Backend::Gpu(_) = self { return true; }
        false
    }
}

// ─────────────────────────────────────────
// GpuBuffer 메서드
// GPU feature 없으면 이 블록은 존재하지 않음
// ─────────────────────────────────────────

#[cfg(feature = "gpu")]
impl GpuBuffer {
    /// CPU Vec<f64> → VRAM
    /// 이 함수가 유일한 CPU→GPU 전송 경로
    pub fn from_cpu_float(dev: &GpuDevice, data: &[f64]) -> Self {
        use wgpu::util::DeviceExt;
        let bytes = bytemuck::cast_slice(data);
        let buffer = dev.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label:    Some("JArray Float"),
                contents: bytes,
                usage:    wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST,
            }
        );
        GpuBuffer {
            count:     data.len(),
            elem_type: GpuElemType::Float,
            inner: GpuBufferInner {
                buffer,
                size: (data.len() * std::mem::size_of::<f64>()) as u64,
            },
        }
    }

    /// CPU Vec<f64> flat [r0,i0,...] → VRAM (Complex)
    pub fn from_cpu_complex(dev: &GpuDevice, data: &[f64]) -> Self {
        use wgpu::util::DeviceExt;
        let bytes = bytemuck::cast_slice(data);
        let buffer = dev.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label:    Some("JArray Complex"),
                contents: bytes,
                usage:    wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST,
            }
        );
        GpuBuffer {
            count:     data.len() / 2,   // 쌍의 수
            elem_type: GpuElemType::Complex,
            inner: GpuBufferInner {
                buffer,
                size: (data.len() * std::mem::size_of::<f64>()) as u64,
            },
        }
    }

    /// VRAM → CPU Vec<f64>
    /// 출력이 필요할 때만 호출 - 비용이 큼
    pub fn to_cpu(&self, dev: &GpuDevice) -> Vec<f64> {
        // staging buffer를 만들어서 GPU→CPU 복사
        let staging = dev.device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("staging"),
            size:               self.inner.size,
            usage:              wgpu::BufferUsages::MAP_READ
                              | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = dev.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: None }
        );
        encoder.copy_buffer_to_buffer(
            &self.inner.buffer, 0,
            &staging,           0,
            self.inner.size,
        );
        dev.queue.submit(std::iter::once(encoder.finish()));

        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| { tx.send(r).unwrap(); });
        dev.device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().unwrap();

        let data = slice.get_mapped_range();
        bytemuck::cast_slice(&data).to_vec()
    }
}

// ─────────────────────────────────────────
// GpuDevice 초기화
// ─────────────────────────────────────────

#[cfg(feature = "gpu")]
impl GpuDevice {
    /// GPU 디바이스 초기화
    /// 실패하면 None 반환 → CPU 폴백
    pub fn try_new() -> Option<Self> {
        pollster::block_on(Self::try_new_async())
    }

    async fn try_new_async() -> Option<GpuDevice> {
        let instance = wgpu::Instance::default();
        let adapter  = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            }
        ).await?;

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                required_features: wgpu::Features::empty(),
                required_limits:   wgpu::Limits::default(),
                label:             None,
            },
            None,
        ).await.ok()?;

        // 커널 컴파일 (WGSL 셰이더)
        let kernels = GpuKernels::compile(&device);

        Some(GpuDevice { device, queue, kernels })
    }
}

// ─────────────────────────────────────────
// GPU 커널 (WGSL 셰이더)
// ─────────────────────────────────────────

#[cfg(feature = "gpu")]
impl GpuKernels {
    fn compile(device: &wgpu::Device) -> Self {
        // float element-wise 이항 연산
        // op_code: 0=add, 1=sub, 2=mul, 3=div
        let float_binop = device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label:  Some("float_binop"),
                layout: None,
                module: &device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label:  Some("float_binop"),
                    source: wgpu::ShaderSource::Wgsl(FLOAT_BINOP_WGSL.into()),
                }),
                entry_point: "main",
            }
        );

        // complex element-wise 이항 연산
        let complex_binop = device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label:  Some("complex_binop"),
                layout: None,
                module: &device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label:  Some("complex_binop"),
                    source: wgpu::ShaderSource::Wgsl(COMPLEX_BINOP_WGSL.into()),
                }),
                entry_point: "main",
            }
        );

        // reduction: sum
        let reduce_sum = device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label:  Some("reduce_sum"),
                layout: None,
                module: &device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label:  Some("reduce_sum"),
                    source: wgpu::ShaderSource::Wgsl(REDUCE_SUM_WGSL.into()),
                }),
                entry_point: "main",
            }
        );

        // reduction: product
        let reduce_prod = device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label:  Some("reduce_prod"),
                layout: None,
                module: &device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label:  Some("reduce_prod"),
                    source: wgpu::ShaderSource::Wgsl(REDUCE_PROD_WGSL.into()),
                }),
                entry_point: "main",
            }
        );

        GpuKernels { float_binop, complex_binop, reduce_sum, reduce_prod }
    }
}

// ─────────────────────────────────────────
// WGSL 셰이더 소스
// ─────────────────────────────────────────

/// float element-wise 이항 연산
/// 각 스레드가 원소 하나를 처리 → leading 크기만큼 병렬
///
/// rank agreement는 CPU에서 이미 처리됨
/// 여기서는 항상 동일한 크기의 두 버퍼를 element-wise 처리
#[cfg(feature = "gpu")]
const FLOAT_BINOP_WGSL: &str = r#"
// push constant로 연산 코드 전달
// 0 = add, 1 = sub, 2 = mul, 3 = div
struct Params {
    count:   u32,
    op_code: u32,
}

@group(0) @binding(0) var<storage, read>       a:      array<f64>;
@group(0) @binding(1) var<storage, read>       w:      array<f64>;
@group(0) @binding(2) var<storage, read_write> result: array<f64>;
@group(0) @binding(3) var<uniform>             params: Params;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i >= params.count { return; }

    result[i] = switch params.op_code {
        case 0u: { a[i] + w[i] }
        case 1u: { a[i] - w[i] }
        case 2u: { a[i] * w[i] }
        case 3u: { a[i] / w[i] }
        default: { 0.0 }
    };
}
"#;

/// complex element-wise 이항 연산
/// flat [r0,i0,r1,i1,...] 형식
/// 각 스레드가 복소수 하나(r,i 쌍)를 처리
#[cfg(feature = "gpu")]
const COMPLEX_BINOP_WGSL: &str = r#"
struct Params {
    count:   u32,   // 복소수 쌍의 수
    op_code: u32,   // 0=add, 1=sub, 2=mul, 3=div
}

@group(0) @binding(0) var<storage, read>       a:      array<f64>;
@group(0) @binding(1) var<storage, read>       w:      array<f64>;
@group(0) @binding(2) var<storage, read_write> result: array<f64>;
@group(0) @binding(3) var<uniform>             params: Params;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i >= params.count { return; }

    let ar = a[i * 2u];
    let ai = a[i * 2u + 1u];
    let wr = w[i * 2u];
    let wi = w[i * 2u + 1u];

    var rr: f64;
    var ri: f64;

    switch params.op_code {
        case 0u: {          // add
            rr = ar + wr;
            ri = ai + wi;
        }
        case 1u: {          // sub
            rr = ar - wr;
            ri = ai - wi;
        }
        case 2u: {          // mul: (ar+ai*j)(wr+wi*j)
            rr = ar*wr - ai*wi;
            ri = ar*wi + ai*wr;
        }
        case 3u: {          // div: a/w = a * conj(w) / |w|²
            let d = wr*wr + wi*wi;
            rr = (ar*wr + ai*wi) / d;
            ri = (ai*wr - ar*wi) / d;
        }
        default: {
            rr = 0.0;
            ri = 0.0;
        }
    }

    result[i * 2u]      = rr;
    result[i * 2u + 1u] = ri;
}
"#;

/// parallel reduction: sum
/// 각 workgroup이 부분합을 계산
/// J의 +/ 에 해당
///
/// 알고리즘: tree reduction
///   step 1: 각 workgroup이 64원소를 더함
///   step 2: workgroup 결과들을 다시 더함 (CPU에서)
///
/// 대규모 배열은 multi-pass가 필요하지만
/// 지금은 단순화
#[cfg(feature = "gpu")]
const REDUCE_SUM_WGSL: &str = r#"
struct Params { count: u32 }

@group(0) @binding(0) var<storage, read>       input:  array<f64>;
@group(0) @binding(1) var<storage, read_write> output: array<f64>;
@group(0) @binding(2) var<uniform>             params: Params;

var<workgroup> shared: array<f64, 64>;

@compute @workgroup_size(64)
fn main(
    @builtin(global_invocation_id) gid:  vec3<u32>,
    @builtin(local_invocation_id)  lid:  vec3<u32>,
    @builtin(workgroup_id)         wgid: vec3<u32>,
) {
    let i = gid.x;
    shared[lid.x] = select(0.0, input[i], i < params.count);
    workgroupBarrier();

    // tree reduction within workgroup
    var stride = 32u;
    loop {
        if stride == 0u { break; }
        if lid.x < stride {
            shared[lid.x] += shared[lid.x + stride];
        }
        workgroupBarrier();
        stride >>= 1u;
    }

    if lid.x == 0u {
        output[wgid.x] = shared[0];
    }
}
"#;

#[cfg(feature = "gpu")]
const REDUCE_PROD_WGSL: &str = r#"
struct Params { count: u32 }

@group(0) @binding(0) var<storage, read>       input:  array<f64>;
@group(0) @binding(1) var<storage, read_write> output: array<f64>;
@group(0) @binding(2) var<uniform>             params: Params;

var<workgroup> shared: array<f64, 64>;

@compute @workgroup_size(64)
fn main(
    @builtin(global_invocation_id) gid:  vec3<u32>,
    @builtin(local_invocation_id)  lid:  vec3<u32>,
    @builtin(workgroup_id)         wgid: vec3<u32>,
) {
    let i = gid.x;
    shared[lid.x] = select(1.0, input[i], i < params.count);
    workgroupBarrier();

    var stride = 32u;
    loop {
        if stride == 0u { break; }
        if lid.x < stride {
            shared[lid.x] *= shared[lid.x + stride];
        }
        workgroupBarrier();
        stride >>= 1u;
    }

    if lid.x == 0u {
        output[wgid.x] = shared[0];
    }
}
"#;
