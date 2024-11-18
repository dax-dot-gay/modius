use std::error::Error;

use async_channel::{Receiver, Sender};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct CommandWrapper {
    pub command: CommandKind,
    pub response: Sender<Result<Value, Box<dyn Error>>>
}

impl CommandWrapper {
    pub fn kind(&self) -> CommandKind {
        self.command.clone()
    }

    pub async fn respond<T: Serialize + DeserializeOwned>(&self, result: Result<T, Box<dyn Error>>) -> Result<(), serde_json::Error> {
        if let Ok(val) = result {
            let _ = self.response.send(Ok(serde_json::to_value(val)?)).await;
        } else if let Err(e) = result {
            let _ = self.response.send(Err(e)).await;
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum CommandKind {
    GetAddress
}

impl CommandKind {
    pub fn wrap(&self) -> (CommandWrapper, Receiver<Result<Value, Box<dyn Error>>>) {
        let (tx, rx) = async_channel::bounded::<Result<Value, Box<dyn Error>>>(1);
        return (
            CommandWrapper {
                command: self.clone(),
                response: tx
            },
            rx
        );
    }

    pub async fn send<T: Serialize + DeserializeOwned>(&self, tx: Sender<CommandWrapper>) -> Result<T, Box<dyn Error>> {
        let (wrapped, rx) = self.wrap();
        tx.send(wrapped).await?;

        match rx.recv().await? {
            Ok(value) => Ok(serde_json::from_value::<T>(value)?),
            Err(error) => Err(error)
        }
    }
}