use tonic::{transport::Channel, Request};

pub mod example {
    tonic::include_proto!("api"); // Include generated code
}

use example::{greeter_client::GreeterClient, HelloRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let channel = Channel::from_static("http://127.0.0.1:50051").connect().await?; // Replace with your server address
    let mut client = GreeterClient::new(channel);

    let request = Request::new(HelloRequest {
        name: "World".into(),
    });

    let response = client.say_hello(request).await?;

    println!("Response: {}", response.into_inner().message);

    Ok(())
}
