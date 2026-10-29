use acp_remote::ipc::{self,LocalIpcAddress};
use std::io::{BufRead,Write};
#[tokio::main]
async fn main()->Result<(),Box<dyn std::error::Error>> {
 let data=std::env::args().nth(1).ok_or("data argument")?;
 let address=LocalIpcAddress::for_data_directory(std::path::Path::new(&data))?;
 if std::env::args().nth(2).as_deref()==Some("--address") {println!("{}",address.pipe_name);return Ok(())}
 let (mut session,start)=ipc::begin_pairing(&address).await?;
 println!("{}",serde_json::to_string(&start)?);std::io::stdout().flush()?;
 let event=session.next_event().await?;
 if event.event!="pending_confirmation" {return Err("not pending".into())}
 let mut line=String::new();std::io::stdin().lock().read_line(&mut line)?;
 if line.trim()!="approve" {return Err("decision missing".into())}
 session.approve().await?;let approved=session.next_event().await?;
 if approved.event!="approved" {return Err("not approved".into())}
 println!("{}",serde_json::to_string(&approved)?);std::io::stdout().flush()?;Ok(())
}
