use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Document {
    pub version: i32,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Documents {
    documents: HashMap<String, Document>,
}

impl Documents {
    pub fn open(&mut self, uri: String, version: i32, text: String) {
        self.documents.insert(uri, Document { version, text });
    }

    pub fn change(&mut self, uri: &str, version: i32, text: String) -> bool {
        let Some(document) = self.documents.get_mut(uri) else {
            return false;
        };
        document.version = version;
        document.text = text;
        true
    }

    pub fn close(&mut self, uri: &str) -> Option<Document> {
        self.documents.remove(uri)
    }

    pub fn get(&self, uri: &str) -> Option<&Document> {
        self.documents.get(uri)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_change_and_close() {
        let mut documents = Documents::default();
        documents.open("file:///a.scad".into(), 1, "cube(1);".into());
        assert_eq!(documents.get("file:///a.scad").unwrap().version, 1);
        assert!(documents.change("file:///a.scad", 2, "sphere(1);".into()));
        assert_eq!(documents.get("file:///a.scad").unwrap().text, "sphere(1);");
        assert!(documents.close("file:///a.scad").is_some());
        assert!(documents.get("file:///a.scad").is_none());
    }
}
