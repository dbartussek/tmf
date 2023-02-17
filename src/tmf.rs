#[repr(u16)]
#[derive(Debug)]
pub(crate) enum SectionType {
    Invalid = 0,
    VertexSegment = 1,
    VertexTriangleSegment = 2,
    NormalSegment = 3,
    NormalTriangleSegment = 4,
    UvSegment = 5,
    UvTriangleSegment = 6,
    MaterialInfo = 7,
    Materialtriangles = 8,
}
impl SectionType {
    pub fn from_u16(input: u16) -> Self {
        match input {
            1 => Self::VertexSegment,
            2 => Self::VertexTriangleSegment,
            3 => Self::NormalSegment,
            4 => Self::NormalTriangleSegment,
            5 => Self::UvSegment,
            6 => Self::UvTriangleSegment,
            _ => Self::Invalid,
        }
    }
}
use crate::{
    FloatType, IndexType, TMFMesh, TMFPrecisionInfo, Vector3, MIN_TMF_MAJOR, MIN_TMF_MINOR,
    TMF_MAJOR, TMF_MINOR,
};
fn calc_shortest_edge(
    vertex_triangles: Option<&[IndexType]>,
    vertices: Option<&[Vector3]>,
) -> Result<FloatType> {
    let shortest_edge = match vertex_triangles {
        Some(vertex_triangles) => {
            use crate::utilis::distance;
            let vertices =
                match vertices {
                    Some(vertices) => vertices,
                    None => return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Saving a mesh with triangle vertex array without normal array is an error.",
                    )),
                };
            let mut shortest_edge = FloatType::INFINITY;
            for i in 0..(vertex_triangles.len() / 3) {
                let d1 = distance(
                    vertices[vertex_triangles[i * 3] as usize],
                    vertices[vertex_triangles[i * 3 + 1] as usize],
                );
                let d2 = distance(
                    vertices[vertex_triangles[i * 3 + 1] as usize],
                    vertices[vertex_triangles[i * 3 + 2] as usize],
                );
                let d3 = distance(
                    vertices[vertex_triangles[i * 3 + 2] as usize],
                    vertices[vertex_triangles[i * 3] as usize],
                );
                shortest_edge = shortest_edge.min(d1.min(d2.min(d3)));
            }
            shortest_edge
        }
        // TODO: Calculate distance between closest points for point cloud
        None => 1.0,
    };
    Ok(shortest_edge)
}
fn save_normals<W: Write>(
    normals: Option<Vec<Vector3>>,
    w: &mut W,
    curr_segment_data: &mut Vec<u8>,
    p_info: &TMFPrecisionInfo,
) -> Result<()> {
    // Save Normals
    match normals {
        Some(normals) => {
            use crate::normals::*;
            save_normal_array(
                &normals,
                curr_segment_data,
                p_info.normal_precision,
            )?;
            w.write_all(&(SectionType::NormalSegment as u16).to_le_bytes())?;
            w.write_all(&(curr_segment_data.len() as u64).to_le_bytes())?;
            w.write_all(&curr_segment_data)?;
            curr_segment_data.clear();
        }
        None => (),
    };
    Ok(())
}
fn save_vertices<W: Write>(
    vertices: Option<&[Vector3]>,
    w: &mut W,
    curr_segment_data: &mut Vec<u8>,
    p_info: &TMFPrecisionInfo,
    shortest_edge: FloatType,
) -> Result<()> {
    match vertices {
        Some(vertices) => {
            use crate::vertices::save_tmf_vertices;
            save_tmf_vertices(
                &vertices,
                p_info.vertex_precision,
                curr_segment_data,
                shortest_edge,
            )?;
            w.write_all(&(SectionType::VertexSegment as u16).to_le_bytes())?;
            w.write_all(&(curr_segment_data.len() as u64).to_le_bytes())?;
            w.write_all(&curr_segment_data)?;
            curr_segment_data.clear();
        }
        None => (),
    }
    Ok(())
}
use crate::normals::map_prune;
use std::io::{Read, Result, Write};
pub(crate) fn write_mesh<W: Write>(
    mesh: &TMFMesh,
    w: &mut W,
    p_info: &TMFPrecisionInfo,
    name: &str,
) -> Result<()> {
    write_string(w, name)?;
    w.write_all(&(mesh.get_segment_count() as u16).to_le_bytes())?;
    // If needed, prune redundant normal data.
    let (normals, normal_triangles) = if mesh.get_normals().is_some()
        && mesh.get_normal_triangles().is_some()
        && p_info.prune_normals
    {
        let mut normals: Vec<Vector3> = mesh.get_normals().unwrap().into();
        let mut normal_triangles: Vec<IndexType> = mesh.get_normal_triangles().unwrap().into();
        map_prune(&mut normals, &mut normal_triangles, 0x1_00_00_00, p_info);
        (Some(normals), Some(normal_triangles))
    } else {
        let normals = match mesh.get_normals() {
            Some(normals) => Some(normals.into()),
            None => None,
        };
        let normal_triangles = match mesh.get_normal_triangles() {
            Some(normal_triangles) => Some(normal_triangles.into()),
            None => None,
        };
        (normals, normal_triangles)
    };
    let mut curr_segment_data = Vec::with_capacity(0x100);
    //Calculate shortest edge, or if no edges present, 1.0
    let shortest_edge = calc_shortest_edge(mesh.get_vertex_triangles(), mesh.get_vertices())?;
    // Save vertices
    save_vertices(
        mesh.get_vertices(),
        w,
        &mut curr_segment_data,
        p_info,
        shortest_edge,
    )?;
    // Save vertex triangles
    match &mesh.vertex_triangles {
        Some(vertex_triangles) => {
            use crate::vertices::save_triangles;
            //If saving vertex triangles, vertices must be present, so unwrap can't fail
            let v_count = mesh.vertices.as_ref().unwrap().len();
            save_triangles(vertex_triangles, v_count, &mut curr_segment_data)?;
            w.write_all(&(SectionType::VertexTriangleSegment as u16).to_le_bytes())?;
            w.write_all(&(curr_segment_data.len() as u64).to_le_bytes())?;
            w.write_all(&curr_segment_data)?;
            curr_segment_data.clear();
        }
        None => (),
    };
    // Save Normals
    save_normals(normals, w, &mut curr_segment_data, p_info)?;
    // Save normal triangles
    match normal_triangles {
        Some(normal_triangles) => {
            use crate::vertices::save_triangles;
            //If saving normal triangles, normals must be present, so unwrap can't fail
            let n_count = mesh.normals.as_ref().unwrap().len();
            save_triangles(&normal_triangles, n_count, &mut curr_segment_data)?;
            w.write_all(&(SectionType::NormalTriangleSegment as u16).to_le_bytes())?;
            w.write_all(&(curr_segment_data.len() as u64).to_le_bytes())?;
            w.write_all(&curr_segment_data)?;
            curr_segment_data.clear();
        }
        None => (),
    };
    match &mesh.uvs {
        Some(uvs) => {
            crate::uv::save_uvs(uvs, &mut curr_segment_data, 0.001)?;
            w.write_all(&(SectionType::UvSegment as u16).to_le_bytes())?;
            w.write_all(&(curr_segment_data.len() as u64).to_le_bytes())?;
            w.write_all(&curr_segment_data)?;
            curr_segment_data.clear();
        }
        None => (),
    }
    // Save uv triangles
    match &mesh.uv_triangles {
        Some(uv_triangles) => {
            use crate::vertices::save_triangles;
            //If saving uv triangles, uvs must be present, so unwrap can't fail
            let uv_count = mesh.uvs.as_ref().unwrap().len();
            save_triangles(uv_triangles, uv_count, &mut curr_segment_data)?;
            w.write_all(&(SectionType::UvTriangleSegment as u16).to_le_bytes())?;
            w.write_all(&(curr_segment_data.len() as u64).to_le_bytes())?;
            w.write_all(&curr_segment_data)?;
            curr_segment_data.clear();
        }
        None => (),
    };
    Ok(())
}
pub(crate) fn write_string<W: Write>(w: &mut W, s: &str) -> Result<()> {
    let bytes = s.as_bytes();
    w.write_all(&(bytes.len() as u16).to_le_bytes())?;
    w.write_all(bytes)
}
pub(crate) fn read_u16<R: Read>(r: &mut R) -> Result<u16> {
    let mut tmp = [0; std::mem::size_of::<u16>()];
    r.read_exact(&mut tmp)?;
    Ok(u16::from_le_bytes(tmp))
}
pub(crate) fn read_string<R: Read>(r: &mut R) -> Result<String> {
    let byte_len = read_u16(r)?;
    let mut bytes = vec![0; byte_len as usize];
    r.read(&mut bytes)?;
    match std::str::from_utf8(&bytes) {
        Ok(string) => Ok(string.to_owned()),
        Err(_) => todo!(),
    }
}
pub(crate) fn write_tmf_header<W: Write>(w: &mut W, mesh_count: u32) -> Result<()> {
    w.write_all(b"TMF")?;
    w.write_all(&TMF_MAJOR.to_le_bytes())?;
    w.write_all(&TMF_MINOR.to_le_bytes())?;
    w.write_all(&MIN_TMF_MAJOR.to_le_bytes())?;
    w.write_all(&MIN_TMF_MINOR.to_le_bytes())?;
    w.write_all(&mesh_count.to_le_bytes())
}
pub(crate) fn write<W: Write, S: std::borrow::Borrow<str>>(
    meshes_names: &[(TMFMesh, S)],
    w: &mut W,
    p_info: &TMFPrecisionInfo,
) -> Result<()> {
    write_tmf_header(w, meshes_names.len() as u32)?;
    for (mesh, name) in meshes_names {
        write_mesh(mesh, w, p_info, name.borrow())?;
    }
    Ok(())
}
#[repr(u8)]
enum CompresionType{
    None,
    Ommited,
    UnalignedLZZ,
}
fn read_segment_header<R: Read>(reader: &mut R)->Result<(SectionType,usize)>{
    let seg_type = read_u16(reader)?;
    let seg_type = SectionType::from_u16(seg_type);
    let data_length = {
            let mut tmp = [0; std::mem::size_of::<u64>()];
            reader.read_exact(&mut tmp)?;
            u64::from_le_bytes(tmp)
    };
    Ok((seg_type,data_length as usize))
}
pub fn read_mesh<R: Read>(reader: &mut R) -> Result<(TMFMesh, String)> {
    let mut res = TMFMesh::empty();
    let name = read_string(reader)?;
    let seg_count = read_u16(reader)?;
    for _ in 0..seg_count {
        let (seg_type,data_length) = read_segment_header(reader)?;
        let mut data = vec![0; data_length as usize];
        reader.read_exact(&mut data)?;
        match seg_type {
            SectionType::VertexSegment => {
                use crate::vertices::read_tmf_vertices;
                if res
                    .set_vertices(&read_tmf_vertices(&mut (&data as &[u8]))?)
                    .is_some()
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Only one vertex array can be present in a model.",
                    ));
                }
            }
            SectionType::NormalSegment => {
                use crate::normals::read_normal_array;
                if res
                    .set_normals(&read_normal_array(&mut (&data as &[u8]))?)
                    .is_some()
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Only one normal array can be present in a model.",
                    ));
                }
            }
            SectionType::UvSegment => {
                use crate::uv::read_uvs;
                if res.set_uvs(&read_uvs(&mut (&data as &[u8]))?).is_some() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Only one uv array can be present in a model.",
                    ));
                }
            }
            SectionType::VertexTriangleSegment => {
                use crate::vertices::read_triangles;
                if res
                    .set_vertex_triangles(&read_triangles(&mut (&data as &[u8]))?)
                    .is_some()
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Only one vertex index array(triangle array) can be present in a model.",
                    ));
                }
            }
            SectionType::NormalTriangleSegment => {
                use crate::vertices::read_triangles;
                if res
                    .set_normal_triangles(&read_triangles(&mut (&data as &[u8]))?)
                    .is_some()
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Only one normal index array(triangle array) can be present in a model.",
                    ));
                }
            }
            SectionType::UvTriangleSegment => {
                use crate::vertices::read_triangles;
                if res
                    .set_uv_triangles(&read_triangles(&mut (&data as &[u8]))?)
                    .is_some()
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Only one uv index array(triangle array) can be present in a model.",
                    ));
                }
            }
            _ => (), //Unknown header, ignoring
        }
    }
    //todo!();
    Ok((res, name))
}
pub fn read<R: Read>(reader: &mut R) -> Result<Vec<(TMFMesh, String)>> {
    let mut magic = [0; 3];
    reader.read_exact(&mut magic)?;
    if magic != *b"TMF" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Not a TMF file",
        ));
    }
    // Not used ATM, but can be used for compatiblity in the future.
    let _major = read_u16(reader)?;
    // Not used ATM, but can be used for compatiblity in the future.
    let _minor = read_u16(reader)?;
    // Minimum version of reader required to read
    let min_major = read_u16(reader)?;
    let min_minor = read_u16(reader)?;
    if min_major > TMF_MAJOR {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "TMF file requires newer version of TMF reader",
        ));
    } else if min_major == TMF_MAJOR && min_minor > TMF_MINOR {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "TMF file requires newer version of TMF reader",
        ));
    }
    let mesh_count = {
        let mut tmp = [0; std::mem::size_of::<u32>()];
        reader.read_exact(&mut tmp)?;
        u32::from_le_bytes(tmp)
    };
    let mut meshes = Vec::with_capacity(mesh_count as usize);
    for _ in 0..mesh_count {
        meshes.push(read_mesh(reader)?);
    }
    Ok(meshes.into())
}
