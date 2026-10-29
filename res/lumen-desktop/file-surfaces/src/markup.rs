//! Bounded annotations exported as a new PNG through an exclusive directory entry.
use crate::captured::Unavailable;
use crate::preview::local_uri;
use image::{ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Rgba, RgbaImage};
use serde_json::{Value,json};
use std::ffi::CString;
use std::fs::{self,File,OpenOptions};
use std::io::{BufReader,Seek,Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt,OpenOptionsExt};
use std::path::{Path,PathBuf};
use std::sync::atomic::{AtomicBool,Ordering};

const MAX_STROKES:usize=512;
const MAX_POINTS:usize=8192;
const MAX_PIXELS:u64=32_000_000;
const MAX_RENDER_WORK:f64=64_000_000.0;

fn unavailable(reason:&'static str,message:&'static str)->Unavailable{Unavailable{reason,message}}
fn canceled(cancel:&AtomicBool)->Result<(),Unavailable>{
    if cancel.load(Ordering::SeqCst){Err(unavailable("canceled","Image export canceled"))}else{Ok(())}
}
#[derive(Clone)]
struct Stroke{tool:Tool,color:Rgba<u8>,width:f64,points:Vec<(f64,f64)>}
#[derive(Clone,Copy)]
enum Tool{Pen,Rectangle}
fn number(value:&Value)->Option<f64>{
    if value.is_boolean(){None}else{value.as_f64().filter(|value|value.is_finite())}
}
fn recipe(value:&Value)->Result<Vec<Stroke>,Unavailable>{
    let invalid=||unavailable("invalid_request","Invalid image annotation");
    let strokes=value.as_array().filter(|strokes|!strokes.is_empty()&&strokes.len()<=MAX_STROKES)
        .ok_or_else(||unavailable("invalid_request","Add at least one annotation before saving"))?;
    let mut total=0;let mut parsed=Vec::new();
    for stroke in strokes{
        let fields=stroke.as_object().ok_or_else(invalid)?;
        if fields.len()!=4||!["tool","color","width","points"].iter().all(|key|fields.contains_key(*key)){return Err(invalid());}
        let tool=match stroke["tool"].as_str(){Some("pen")=>Tool::Pen,Some("rectangle")=>Tool::Rectangle,_=>return Err(invalid())};
        let raw=stroke["color"].as_str().ok_or_else(invalid)?.as_bytes();
        if raw.len()!=7||raw[0]!=b'#'||!raw[1..].iter().all(u8::is_ascii_hexdigit){return Err(invalid());}
        let hex=|index|u8::from_str_radix(std::str::from_utf8(&raw[index..index+2]).unwrap(),16).unwrap();
        let color=Rgba([hex(1),hex(3),hex(5),255]);
        let width=number(&stroke["width"]).filter(|width|(0.001..=0.05).contains(width)).ok_or_else(invalid)?;
        let points=stroke["points"].as_array().filter(|points|points.len()>=2&&points.len()<=MAX_POINTS
            && (!matches!(tool,Tool::Rectangle)||points.len()==2)).ok_or_else(invalid)?;
        total+=points.len();if total>MAX_POINTS{return Err(unavailable("policy_limit","This annotation has too many points"));}
        let points=points.iter().map(|point|{
            let pair=point.as_array().filter(|pair|pair.len()==2).ok_or_else(||unavailable("invalid_request","Annotation points must lie inside the image"))?;
            let x=number(&pair[0]).filter(|value|(0.0..=1.0).contains(value));
            let y=number(&pair[1]).filter(|value|(0.0..=1.0).contains(value));
            x.zip(y).ok_or_else(||unavailable("invalid_request","Annotation points must lie inside the image"))
        }).collect::<Result<Vec<_>,_>>()?;
        parsed.push(Stroke{tool,color,width,points});
    }
    Ok(parsed)
}
fn open_resource(resource:&Path)->Result<File,Unavailable>{
    let file=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK|libc::O_CLOEXEC)
        .open(resource).map_err(|_|unavailable("source_changed","The retained image is unavailable"))?;
    if !file.metadata().is_ok_and(|info|info.is_file()){return Err(unavailable("source_changed","The retained image is unavailable"));}
    Ok(file)
}
fn inspect(resource:&Path)->Result<(File,ImageFormat,u32,u32,u8),Unavailable>{
    let file=open_resource(resource)?;
    let reader=ImageReader::new(BufReader::new(file.try_clone().map_err(|_|unavailable("source_changed","The retained image is unavailable"))?))
        .with_guessed_format().map_err(|_|unavailable("unsupported","This image cannot be decoded for markup"))?;
    let format=reader.format().filter(|format|matches!(format,ImageFormat::Png|ImageFormat::Jpeg|ImageFormat::WebP))
        .ok_or_else(||unavailable("unsupported","Markup supports still PNG, JPEG and WebP images"))?;
    if format==ImageFormat::WebP{
        let decoder=image::codecs::webp::WebPDecoder::new(BufReader::new(open_resource(resource)?))
            .map_err(|_|unavailable("unsupported","This image cannot be decoded for markup"))?;
        if decoder.has_animation(){return Err(unavailable("unsupported","Markup supports still PNG, JPEG and WebP images"));}
    }
    let mut decoder=reader.into_decoder().map_err(|_|unavailable("unsupported","This image cannot be decoded for markup"))?;
    let (width,height)=decoder.dimensions();
    if width as u64*height as u64>MAX_PIXELS{return Err(unavailable("policy_limit","Markup supports images up to 32 million pixels"));}
    let orientation=decoder.orientation().map_err(|_|unavailable("unsupported","This image cannot be decoded for markup"))?.to_exif();
    Ok((file,format,width,height,orientation))
}
pub fn describe(resource:&Path)->Result<Value,Unavailable>{
    let (_,format,width,height,orientation)=inspect(resource)?;
    let format=match format{ImageFormat::Png=>"PNG",ImageFormat::Jpeg=>"JPEG",ImageFormat::WebP=>"WEBP",_=>unreachable!()};
    Ok(json!({"supported":true,"width":width,"height":height,"format":format,"orientation":orientation}))
}
fn plan(strokes:&[Stroke],size:(u32,u32))->Result<Vec<(Tool,Rgba<u8>,Vec<(i32,i32)>,i32)>,Unavailable>{
    let mut work=0.0;let mut result=Vec::new();
    for stroke in strokes{
        let points=stroke.points.iter().map(|(x,y)|((x*(size.0-1) as f64).round() as i32,(y*(size.1-1) as f64).round() as i32)).collect::<Vec<_>>();
        let width=(stroke.width*size.0.min(size.1) as f64).round().max(1.0) as i32;
        let cost=match stroke.tool{
            Tool::Rectangle=>{let (a,b)=(points[0],points[1]);(2*((b.0-a.0).abs()+(b.1-a.1).abs())*width+4*width*width) as f64},
            Tool::Pen=>points.windows(2).map(|pair|{let (a,b)=(pair[0],pair[1]);
                (((b.0-a.0) as f64).hypot((b.1-a.1) as f64)+width as f64)*width as f64}).sum::<f64>()+(width*width) as f64,
        };
        work+=cost;if work>MAX_RENDER_WORK{return Err(unavailable("policy_limit","This drawing is too complex at this image size; undo or use fewer strokes"));}
        result.push((stroke.tool,stroke.color,points,width));
    }
    Ok(result)
}
fn segment(image:&mut RgbaImage,a:(i32,i32),b:(i32,i32),width:i32,color:Rgba<u8>){
    let dx=(b.0-a.0) as f64;let dy=(b.1-a.1) as f64;
    let length_squared=dx*dx+dy*dy;
    let radius=(width as f64/2.0).max(0.5);
    let horizontal=dx.abs()>=dy.abs();
    let major=if horizontal{(a.0.min(b.0),a.0.max(b.0),image.width())}else{(a.1.min(b.1),a.1.max(b.1),image.height())};
    let extent=(radius+1.0).ceil() as i32;
    let first=(major.0-extent).max(0);
    let last=(major.1+extent).min(major.2 as i32-1);
    let slope=if horizontal{if dx==0.0{0.0}else{dy/dx}}else{if dy==0.0{0.0}else{dx/dy}};
    let band=(radius*(1.0+slope*slope).sqrt()+2.0).ceil() as i32;
    for major_at in first..=last{
        let center=if horizontal{a.1 as f64+slope*(major_at-a.0) as f64}
            else{a.0 as f64+slope*(major_at-a.1) as f64};
        let max_minor=if horizontal{image.height()}else{image.width()};
        for minor_at in ((center as i32-band).max(0))..=((center as i32+band).min(max_minor as i32-1)){
            let (x,y)=if horizontal{(major_at,minor_at)}else{(minor_at,major_at)};
            let projection=if length_squared==0.0{0.0}else{(((x-a.0) as f64*dx+(y-a.1) as f64*dy)/length_squared).clamp(0.0,1.0)};
            let nearest_x=a.0 as f64+projection*dx;let nearest_y=a.1 as f64+projection*dy;
            if ((x as f64-nearest_x).powi(2)+(y as f64-nearest_y).powi(2)).sqrt()<=radius{
                image.put_pixel(x as u32,y as u32,color);
            }
        }
    }
}
fn draw(image:&mut RgbaImage,plan:&[(Tool,Rgba<u8>,Vec<(i32,i32)>,i32)],cancel:&AtomicBool)->Result<(),Unavailable>{
    for (tool,color,points,width) in plan{
        canceled(cancel)?;
        match tool{
            Tool::Pen=>for pair in points.windows(2){segment(image,pair[0],pair[1],*width,*color);},
            Tool::Rectangle=>{let (a,b)=(points[0],points[1]);let (left,right)=(a.0.min(b.0),a.0.max(b.0));
                let (top,bottom)=(a.1.min(b.1),a.1.max(b.1));
                for (start,end) in [((left,top),(right,top)),((right,top),(right,bottom)),
                    ((right,bottom),(left,bottom)),((left,bottom),(left,top))]{segment(image,start,end,*width,*color);}
            }
        }
    }
    Ok(())
}
fn destination(value:&Value)->Result<PathBuf,Unavailable>{
    let raw=value.as_str().filter(|raw|!raw.is_empty()&&raw.len()<=8192&&!raw.contains('\0'))
        .ok_or_else(||unavailable("invalid_request","Choose a new PNG destination"))?;
    let path=if raw.starts_with("file:"){local_uri(raw).map_err(|_|unavailable("invalid_request","Choose a local PNG destination"))?}
        else{PathBuf::from(raw)};
    if !path.is_absolute()||!path.extension().is_some_and(|ext|ext.to_string_lossy().eq_ignore_ascii_case("png"))
        ||path.file_name().is_none(){return Err(unavailable("invalid_request","Save the annotated copy as a new PNG"));}
    Ok(path)
}
fn publish(image:&RgbaImage,target:&Path,original:&Path,cancel:&AtomicBool,validate:&impl Fn()->Result<(),Unavailable>)->Result<Value,Unavailable>{
    canceled(cancel)?;validate()?;
    let requested=target.parent().ok_or_else(||unavailable("destination_unavailable","The destination folder is unavailable; choose another folder"))?;
    let parent=fs::canonicalize(requested).map_err(|_|unavailable("destination_unavailable","The destination folder is unavailable; choose another folder"))?;
    let directory=OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY|libc::O_NOFOLLOW|libc::O_CLOEXEC)
        .open(&parent).map_err(|_|unavailable("destination_unavailable","The destination folder is unavailable; choose another folder"))?;
    let name=target.file_name().unwrap();
    if parent.join(name)==fs::canonicalize(original).unwrap_or_else(|_|original.to_path_buf()){
        return Err(unavailable("target_exists","Choose a different filename; the original is never overwritten"));
    }
    let mut random=[0_u8;16];getrandom::fill(&mut random).map_err(|_|unavailable("provider_failure","Image export unavailable"))?;
    let temporary=format!(".lumen-markup-{}.png",hex::encode(random));
    let temp_path=parent.join(&temporary);
    let result=(||{
        let mut output=OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC)
            .open(&temp_path).map_err(|_|unavailable("permission_denied","Permission to save in that folder was denied"))?;
        image::codecs::png::PngEncoder::new(&mut output).write_image(image.as_raw(),image.width(),image.height(),image::ColorType::Rgba8.into())
            .map_err(|_|unavailable("provider_failure","Image export failed"))?;
        output.flush().and_then(|_|output.sync_all()).map_err(|_|unavailable("provider_failure","Image export failed"))?;
        canceled(cancel)?;validate()?;
        let now=fs::canonicalize(requested).map_err(|_|unavailable("destination_changed","The destination folder changed; choose it again"))?;
        let info=fs::metadata(&now).map_err(|_|unavailable("destination_changed","The destination folder changed; choose it again"))?;
        let opened=directory.metadata().map_err(|_|unavailable("destination_changed","The destination folder changed; choose it again"))?;
        if now!=parent||info.dev()!=opened.dev()||info.ino()!=opened.ino(){return Err(unavailable("destination_changed","The destination folder changed; choose it again"));}
        let source=CString::new(temporary.as_bytes()).unwrap();
        let destination=CString::new(name.as_bytes()).map_err(|_|unavailable("invalid_request","Choose a new PNG destination"))?;
        // SAFETY: both file names are NUL-free and both descriptors refer to the same verified directory.
        let linked=unsafe{libc::linkat(directory.as_raw_fd(),source.as_ptr(),directory.as_raw_fd(),destination.as_ptr(),0)};
        if linked!=0{
            let error=std::io::Error::last_os_error();
            return Err(if error.kind()==std::io::ErrorKind::AlreadyExists{unavailable("target_exists","That filename already exists; choose a new name")}
                else{unavailable("permission_denied","Permission to save in that folder was denied")});
        }
        let warning=if directory.sync_all().is_ok(){""}else{"The copy was saved, but the folder could not acknowledge durable storage"};
        Ok(json!({"saved":true,"path":parent.join(name),"message":"Saved annotated copy","warning":warning}))
    })();
    let _=fs::remove_file(temp_path);
    result
}
pub fn save(resource:&Path,original:&Path,parameters:&Value,cancel:&AtomicBool,validate:&impl Fn()->Result<(),Unavailable>)->Result<Value,Unavailable>{
    let fields=parameters.as_object().ok_or_else(||unavailable("invalid_request","Invalid markup export request"))?;
    if fields.len()!=2||!fields.contains_key("strokes")||!fields.contains_key("destination"){
        return Err(unavailable("invalid_request","Invalid markup export request"));
    }
    let strokes=recipe(&parameters["strokes"])?;let target=destination(&parameters["destination"])?;
    canceled(cancel)?;validate()?;
    let (mut source,_,width,height,_)=inspect(resource)?;
    source.rewind().map_err(|_|unavailable("source_changed","The retained image is unavailable"))?;
    let image=ImageReader::new(BufReader::new(source)).with_guessed_format()
        .map_err(|_|unavailable("unsupported","This image cannot be decoded for markup"))?.decode()
        .map_err(|_|unavailable("unsupported","This image cannot be decoded for markup"))?;
    if image.width()!=width||image.height()!=height{return Err(unavailable("source_changed","The retained image changed"));}
    let planned=plan(&strokes,(width,height))?;
    let mut image=image.to_rgba8();
    draw(&mut image,&planned,cancel)?;
    publish(&image,&target,original,cancel,validate)
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn recipe_bounds_and_pixels(){
        let strokes=json!([{"tool":"pen","color":"#ff0000","width":0.025,"points":[[0.1,0.5],[0.9,0.5]]}]);
        let parsed=recipe(&strokes).unwrap();let plan=plan(&parsed,(100,80)).unwrap();
        let mut image=RgbaImage::new(100,80);draw(&mut image,&plan,&AtomicBool::new(false)).unwrap();
        assert_eq!(image.get_pixel(50,40),&Rgba([255,0,0,255]));
        assert!(recipe(&json!([])).is_err());
        assert!(recipe(&json!([{"tool":"pen","color":"#ff0000","width":true,"points":[[0,0],[1,1]]}])).is_err());
        assert!(recipe(&json!([{"tool":"rectangle","color":"#000000","width":0.01,"points":[[0,0],[1,1],[0.5,0.5]]}])).is_err());
    }
    #[test]
    fn export_writes_only_a_new_png_and_cleans_temporary_files(){
        let root=tempfile::tempdir().unwrap();
        let original=root.path().join("original.png");
        RgbaImage::from_pixel(100,80,Rgba([255,255,255,255])).save(&original).unwrap();
        let before=fs::read(&original).unwrap();
        let strokes=json!([{"tool":"pen","color":"#ff0000","width":0.025,"points":[[0.1,0.5],[0.9,0.5]]}]);
        let request=json!({"destination":root.path().join("copy.png"),"strokes":strokes});
        let cancel=AtomicBool::new(false);
        let saved=save(&original,&original,&request,&cancel,&||Ok(())).unwrap();
        assert_eq!(saved["saved"],true);
        assert_eq!(fs::read(&original).unwrap(),before);
        assert_eq!(image::open(root.path().join("copy.png")).unwrap().to_rgba8().get_pixel(50,40),&Rgba([255,0,0,255]));
        assert!(root.path().read_dir().unwrap().flatten().all(|entry|!entry.file_name().to_string_lossy().starts_with(".lumen-markup-")));
        assert_eq!(save(&original,&original,&request,&cancel,&||Ok(())).unwrap_err().reason,"target_exists");
        let mut same=request.clone();same["destination"]=json!(original);
        assert_eq!(save(&original,&original,&same,&cancel,&||Ok(())).unwrap_err().reason,"target_exists");
    }
    #[test]
    fn cancellation_and_parent_retarget_never_publish(){
        let root=tempfile::tempdir().unwrap();
        let original=root.path().join("original.png");
        RgbaImage::from_pixel(20,20,Rgba([255,255,255,255])).save(&original).unwrap();
        let first=root.path().join("first");let second=root.path().join("second");
        fs::create_dir(&first).unwrap();fs::create_dir(&second).unwrap();
        let alias=root.path().join("alias");std::os::unix::fs::symlink(&first,&alias).unwrap();
        let request=json!({"destination":alias.join("copy.png"),"strokes":[{"tool":"rectangle","color":"#0000ff","width":0.02,"points":[[0.2,0.2],[0.8,0.8]]}]});
        let cancel=AtomicBool::new(true);
        assert_eq!(save(&original,&original,&request,&cancel,&||Ok(())).unwrap_err().reason,"canceled");
        let verify=||{
            if first.read_dir().unwrap().flatten().any(|entry|entry.file_name().to_string_lossy().starts_with(".lumen-markup-")){
                fs::remove_file(&alias).unwrap();std::os::unix::fs::symlink(&second,&alias).unwrap();
            }
            Ok(())
        };
        let error=save(&original,&original,&request,&AtomicBool::new(false),&verify).unwrap_err();
        assert_eq!(error.reason,"destination_changed");
        assert!(first.read_dir().unwrap().next().is_none());
        assert!(second.read_dir().unwrap().next().is_none());
    }
    #[test]
    fn oriented_jpeg_and_webp_keep_raw_pixels_and_reset_output_orientation(){
        let root=tempfile::tempdir().unwrap();
        let mut pixels=image::RgbImage::from_pixel(80,40,image::Rgb([255,255,255]));
        for y in 0..20{for x in 0..40{pixels.put_pixel(x,y,image::Rgb([255,0,0]));}}
        // TIFF little-endian IFD with one Orientation short value (EXIF tag 274).
        let exif=|orientation:u16|{
            let mut value=vec![b'I',b'I',42,0,8,0,0,0,1,0,0x12,0x01,3,0,1,0,0,0];
            value.extend_from_slice(&orientation.to_le_bytes());value.extend_from_slice(&[0,0,0,0,0,0]);value
        };
        for (extension,orientation) in [("jpg",6_u16),("webp",8_u16)]{
            let source=root.path().join(format!("oriented.{extension}"));
            let mut bytes=Vec::new();
            if extension=="jpg"{
                let mut encoder=image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes,95);
                encoder.set_exif_metadata(exif(orientation)).unwrap();
                encoder.write_image(pixels.as_raw(),80,40,image::ColorType::Rgb8.into()).unwrap();
            }else{
                let mut encoder=image::codecs::webp::WebPEncoder::new_lossless(&mut bytes);
                encoder.set_exif_metadata(exif(orientation)).unwrap();
                encoder.write_image(pixels.as_raw(),80,40,image::ColorType::Rgb8.into()).unwrap();
            }
            fs::write(&source,&bytes).unwrap();
            let info=describe(&source).unwrap();
            assert_eq!((info["width"].as_u64(),info["height"].as_u64(),info["orientation"].as_u64()),
                (Some(80),Some(40),Some(orientation as u64)));
            let target=root.path().join(format!("copy-{extension}.png"));
            let request=json!({"destination":target,"strokes":[{"tool":"rectangle","color":"#0000ff","width":0.02,"points":[[0.5,0.5],[0.9,0.9]]}]});
            save(&source,&source,&request,&AtomicBool::new(false),&||Ok(())).unwrap();
            let result=image::open(&target).unwrap().to_rgba8();
            assert_eq!(result.dimensions(),(80,40));
            assert!(result.get_pixel(8,4)[0]>230);
            assert_eq!(describe(&target).unwrap()["orientation"],1);
            assert_eq!(fs::read(&source).unwrap(),bytes);
        }
    }
}
