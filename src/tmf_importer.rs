use crate::tmf::{DecodedSegment, EncodedSegment, SectionType};
use crate::{TMFImportError, TMFMesh, TMF_MAJOR, TMF_MINOR};
use futures::future::join_all;
pub(crate) enum SegLenWidth {
    U32,
    U64,
}
impl SegLenWidth {
    fn from_header(header: &TMFHeader) -> Self {
        if header.min_minor > 1 {
            Self::U32
        } else {
            Self::U64
        }
    }
    pub(crate) fn read<R: std::io::Read>(&self, src: &mut R) -> std::io::Result<usize> {
        Ok(match self {
            Self::U32 => {
                let mut tmp = [0; std::mem::size_of::<u32>()];
                src.read_exact(&mut tmp)?;
                u32::from_le_bytes(tmp) as usize
            }
            Self::U64 => {
                let mut tmp = [0; std::mem::size_of::<u64>()];
                src.read_exact(&mut tmp)?;
                u64::from_le_bytes(tmp) as usize
            }
        })
    }
}
pub(crate) enum SegTypeWidth {
    U16,
    U8,
}
impl SegTypeWidth {
    fn from_header(header: &TMFHeader) -> Self {
        if header.min_minor > 1 {
            Self::U8
        } else {
            Self::U16
        }
    }
    pub(crate) fn read<R: std::io::Read>(&self, src: &mut R) -> std::io::Result<SectionType> {
        Ok(match self {
            Self::U8 => {
                let mut tmp = [0; std::mem::size_of::<u8>()];
                src.read_exact(&mut tmp)?;
                SectionType::from_u8(u8::from_le_bytes(tmp))
            }
            Self::U16 => {
                let mut tmp = [0; std::mem::size_of::<u16>()];
                src.read_exact(&mut tmp)?;
                SectionType::from_u16(u16::from_le_bytes(tmp))
            }
        })
    }
}
pub(crate) struct TMFImportContext {
    slw: SegLenWidth,
    stw: SegTypeWidth,
    should_read_min_index: bool,
    meshes: Vec<TMFMesh>,
}
// While some of those fileds are not read yet, they may be relevant in the future.
#[allow(dead_code)]
struct TMFHeader {
    major: u16,
    minor: u16,
    min_major: u16,
    min_minor: u16,
}
pub(crate) fn read_string<R: std::io::Read>(src: &mut R) -> std::io::Result<String> {
    let byte_len = {
        let mut tmp = [0; std::mem::size_of::<u16>()];
        src.read_exact(&mut tmp)?;
        u16::from_le_bytes(tmp)
    };
    let mut bytes = vec![0; byte_len as usize];
    src.read_exact(&mut bytes)?;
    match std::str::from_utf8(&bytes) {
        Ok(string) => Ok(string.to_owned()),
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Mesh name not valid UTF-8",
        )),
    }
}
async fn read_tmf_header<R: std::io::Read>(src: &mut R) -> Result<TMFHeader, TMFImportError> {
    let mut magic = [0; 3];
    src.read_exact(&mut magic)?;
    if magic != *b"TMF" {
        return Err(TMFImportError::NotTMFFile);
    }
    let major = {
        let mut tmp = [0; std::mem::size_of::<u16>()];
        src.read_exact(&mut tmp)?;
        u16::from_le_bytes(tmp)
    };
    let minor = {
        let mut tmp = [0; std::mem::size_of::<u16>()];
        src.read_exact(&mut tmp)?;
        u16::from_le_bytes(tmp)
    };
    // Minimum version of reader required to read
    let min_major = {
        let mut tmp = [0; std::mem::size_of::<u16>()];
        src.read_exact(&mut tmp)?;
        u16::from_le_bytes(tmp)
    };
    let min_minor = {
        let mut tmp = [0; std::mem::size_of::<u16>()];
        src.read_exact(&mut tmp)?;
        u16::from_le_bytes(tmp)
    };
    if min_major > TMF_MAJOR || (min_major == TMF_MAJOR && min_minor > TMF_MINOR) {
        Err(TMFImportError::NewerVersionRequired)
    } else {
        Ok(TMFHeader {
            major,
            minor,
            min_major,
            min_minor,
        })
    }
}
impl TMFImportContext {
    pub(crate) fn stw(&self) -> &SegTypeWidth {
        &self.stw
    }
    pub(crate) fn slw(&self) -> &SegLenWidth {
        &self.slw
    }
    pub(crate) fn read_traingle_min<R: std::io::Read>(&self, src: &mut R) -> std::io::Result<u64> {
        if self.should_read_min_index {
            let mut tmp = [0; std::mem::size_of::<u64>()];
            src.read_exact(&mut tmp)?;
            Ok(u64::from_le_bytes(tmp))
        } else {
            Ok(0)
        }
    }
    fn init_header(hdr: TMFHeader) -> Self {
        let slw = SegLenWidth::from_header(&hdr);
        let stw = SegTypeWidth::from_header(&hdr);
        Self {
            slw,
            stw,
            meshes: Vec::new(),
            should_read_min_index: (hdr.min_minor > 1),
        }
    }
    async fn import_mesh<R: std::io::Read>(
        &self,
        mut src: R,
        ctx: &crate::tmf_importer::TMFImportContext,
    ) -> Result<(TMFMesh, String), TMFImportError> {
        let name = read_string(&mut src)?;
        let segment_count = {
            let mut tmp = [0; std::mem::size_of::<u16>()];
            src.read_exact(&mut tmp)?;
            u16::from_le_bytes(tmp)
        }; //self.slw.read(&mut src)?;
        let mut decoded_segs = Vec::with_capacity(segment_count as usize);
        for _ in 0..segment_count {
            let encoded = EncodedSegment::read(self, &mut src)?;
            decoded_segs.push(DecodedSegment::decode(encoded, ctx));
        }
        let mut res = TMFMesh::empty();
        join_all(decoded_segs)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .for_each(|seg| {
                seg.apply(&mut res);
            });
        Ok((res, name))
    }
    pub(crate) async fn import<R: std::io::Read>(
        mut src: R,
    ) -> Result<Vec<(TMFMesh, String)>, TMFImportError> {
        let header = read_tmf_header(&mut src).await?;
        let res = Self::init_header(header);
        let mesh_count = {
            let mut tmp = [0; std::mem::size_of::<u32>()];
            src.read_exact(&mut tmp)?;
            u32::from_le_bytes(tmp)
        };
        let mut meshes = Vec::with_capacity((u16::MAX as usize).min(mesh_count as usize));
        for _ in 0..mesh_count {
            meshes.push(res.import_mesh(&mut src, &res).await?);
        }
        Ok(meshes)
    }
}
pub(crate) fn import_sync<R: std::io::Read>(
    src: R,
) -> Result<Vec<(TMFMesh, String)>, TMFImportError> {
    futures::executor::block_on(TMFImportContext::import(src))
}
#[cfg(test)]
fn init_test_env() {
    std::fs::create_dir_all("target/test_res").unwrap();
}
#[cfg(test)]
#[test]
fn test() {
    use crate::TMFPrecisionInfo;

    init_test_env();
    let mut file = std::fs::File::open("testing/susan.obj").unwrap();
    let (tmf_mesh, name) = TMFMesh::read_from_obj_one(&mut file).unwrap();
    tmf_mesh.verify().unwrap();
    assert!(name == "Suzanne", "Name should be Suzanne but is {name}");
    let prec = TMFPrecisionInfo::default();
    let mut out = Vec::new();
    {
        tmf_mesh.write_tmf_one(&mut out, &prec, name).unwrap();
    }
    let _imported = futures::executor::block_on(TMFImportContext::import(&out[..])).unwrap();
}
#[cfg(test)]
#[test]
fn test_triangles_opt() {
    use crate::tmf_exporter::EncodeInfo;
    use crate::TMFPrecisionInfo;
    use futures::executor::block_on;
    let mut tmp = Vec::with_capacity(1_000_000);
    for i in 0..1000 {
        tmp.push(i);
    }
    let tris = DecodedSegment::AppendTriangleVertex(tmp.into());
    let tris = block_on(tris.optimize());
    let tris: Vec<EncodedSegment> = tris
        .into_iter()
        .map(|seg| {
            block_on(seg.encode(&TMFPrecisionInfo::default(), &EncodeInfo::default())).unwrap()
        })
        .collect();
    let ctx = TMFImportContext::init_header(TMFHeader {
        major: crate::TMF_MAJOR,
        minor: crate::TMF_MINOR,
        min_major: crate::MIN_TMF_MAJOR,
        min_minor: crate::MIN_TMF_MINOR,
    });
    let tris: Vec<DecodedSegment> = tris
        .into_iter()
        .map(|seg| {
            let seg: DecodedSegment = block_on(DecodedSegment::decode(seg, &ctx)).unwrap();
            seg
        })
        .collect();
    println!("{tris:?}");
    let mut curr = 0;
    for seg in tris.iter() {
        let values = if let DecodedSegment::AppendTriangleVertex(vals) = seg {
            vals
        } else {
            panic!()
        };
        for value in values.iter() {
            assert_eq!(*value, curr);
            curr += 1;
        }
    }
}