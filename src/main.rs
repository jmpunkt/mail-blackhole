//!

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use tokio::sync::broadcast;

    let args: mail_blackhole::Args = argh::from_env();

    println!("using configuration: {:?}", args);

    let (sender, _) = broadcast::channel(16);

    tokio::select! {
        val = mail_blackhole::http::listen(&args, sender.clone()) => {
            if let Err(err) = val {
                println!("http server failed: {}", err);
            } else {
                println!("http server finished");
            }
        }
        val = mail_blackhole::mail::listen(&args, sender) => {
            if let Err(err) = val {
                println!("mail server failed: {}", err);
            } else {
                println!("mail server finished");
            }
        }
    }

    Ok(())
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
