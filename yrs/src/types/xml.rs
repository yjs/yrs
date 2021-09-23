use crate::block::{Item, ItemContent, ItemPosition, Prelim};
use crate::types::{Branch, BranchRef, Entries, Map, Text, TypePtr, Value, TYPE_REFS_XML_ELEMENT, TYPE_REFS_XML_TEXT, TYPE_REFS_XML_FRAGMENT};
use crate::Transaction;
use lib0::any::Any;
use std::cell::Ref;
use std::fmt::Write;

/// An return type from XML elements retrieval methods. It's an enum of all supported values, that
/// can be nested inside of [XmlElement]. These are other [XmlElement]s or [XmlText] values.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Xml {
    Element(XmlElement),
    Text(XmlText),
}

impl From<BranchRef> for Xml {
    fn from(inner: BranchRef) -> Self {
        let type_ref = { inner.borrow().type_ref & 0b1111 };
        match type_ref {
            TYPE_REFS_XML_ELEMENT => Xml::Element(XmlElement::from(inner)),
            TYPE_REFS_XML_TEXT => Xml::Text(XmlText::from(inner)),
            other => panic!("Unsupported type: {}", other),
        }
    }
}

/// XML element data type. It represents an XML node, which can contain key-value attributes
/// (interpreted as strings) as well as other nested XML elements or rich text (represented by
/// [XmlText] type).
///
/// In terms of conflict resolution, [XmlElement] uses following rules:
///
/// - Attribute updates use logical last-write-wins principle, meaning the past updates are
///   automatically overridden and discarded by newer ones, while concurrent updates made by
///   different peers are resolved into a single value using document id seniority to establish
///   an order.
/// - Child node insertion uses sequencing rules from other Yrs collections - elements are inserted
///   using interleave-resistant algorithm, where order of concurrent inserts at the same index
///   is established using peer's document id seniority.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct XmlElement(XmlFragment);

impl XmlElement {
    fn inner(&self) -> Ref<Branch> {
        self.0.inner()
    }

    /// Converts current XML node into a textual representation. This representation if flat, it
    /// doesn't include any indentation.
    pub fn to_string(&self, txn: &Transaction) -> String {
        let inner = self.inner();
        let mut s = String::new();
        let tag = inner
            .name
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or(&"UNDEFINED");
        write!(&mut s, "<{}", tag).unwrap();
        let attributes = Attributes(inner.entries(txn));
        for (k, v) in attributes {
            write!(&mut s, " \"{}\"=\"{}\"", k, v).unwrap();
        }
        write!(&mut s, ">").unwrap();
        for i in inner.iter(txn) {
            for content in i.content.get_content(txn) {
                write!(&mut s, "{}", content.to_string(txn)).unwrap();
            }
        }
        write!(&mut s, "</{}>", tag).unwrap();
        s
    }

    /// A tag name of a current top-level XML node, eg. node `<p></p>` has "p" as it's tag name.
    pub fn tag(&self) -> &str {
        let inner = self.0 .0.as_ref();
        inner.name.as_ref().unwrap()
    }

    /// Removes an attribute recognized by an `attr_name` from a current XML element.
    pub fn remove_attribute(&self, txn: &mut Transaction, attr_name: &str) {
        self.inner().remove(txn, attr_name);
    }

    /// Inserts an attribute entry into current XML element.
    pub fn insert_attribute<K: ToString, V: ToString>(
        &self,
        txn: &mut Transaction,
        attr_name: K,
        attr_value: V,
    ) {
        let key = attr_name.to_string();
        let value = crate::block::PrelimText(attr_value.to_string());
        let pos = {
            let inner = self.inner();
            let left = inner.map.get(&key);
            ItemPosition {
                parent: inner.ptr.clone(),
                left: left.cloned(),
                right: None,
                index: 0,
            }
        };

        txn.create_item(&pos, value, Some(key));
    }

    /// Returns a value of an attribute given its `attr_name`. Returns `None` if no such attribute
    /// can be found inside of a current XML element.
    pub fn get_attribute(&self, txn: &Transaction, attr_name: &str) -> Option<String> {
        let inner: Ref<_> = self.inner();
        let value = inner.get(txn, attr_name)?;
        Some(value.to_string(txn))
    }

    /// Returns an unordered iterator over all attributes (key-value pairs), that can be found
    /// inside of a current XML element.
    pub fn attributes<'a, 'b, 'txn>(&'a self, txn: &'b Transaction<'txn>) -> Attributes<'b, 'txn> {
        let inner = self.inner();
        let blocks = inner.entries(txn);
        Attributes(blocks)
    }

    /// Returns a next sibling of a current XML element, if any exists.
    pub fn next_sibling(&self, txn: &Transaction) -> Option<Xml> {
        next_sibling(self.inner(), txn)
    }

    /// Returns a previous sibling of a current XML element, if any exists.
    pub fn prev_sibling(&self, txn: &Transaction) -> Option<Xml> {
        prev_sibling(self.inner(), txn)
    }

    /// Returns a parent XML element, current node can be found within.
    /// Returns `None`, if current node is a root.
    pub fn parent(&self, txn: &Transaction) -> Option<XmlElement> {
        self.0.parent(txn)
    }

    /// Returns a first child XML node (either [XmlElement] or [XmlText]), that can be found in
    /// a current XML element. Returns `None` if current element is empty.
    pub fn first_child(&self, txn: &Transaction) -> Option<Xml> {
        self.0.first_child(txn)
    }

    /// Returns a number of child XML nodes, that can be found inside of a current XML element.
    /// This is a flat count - successor nodes (children of a children) are not counted.
    pub fn len(&self, txn: &Transaction) -> u32 {
        self.0.len(txn)
    }

    /// Returns an iterator that can be used to traverse over the successors of a current
    /// XML element. This includes recursive step over children of its children. The recursive
    /// iteration is depth-first.
    ///
    /// Example:
    /// ```
    /// /* construct node with a shape:
    ///    <div>
    ///       <p>Hello <b>world</b></p>
    ///       again
    ///    </div>
    /// */
    /// use yrs::{XmlElement, Doc, Xml};
    ///
    /// let doc = Doc::new();
    /// let mut txn = doc.transact();
    /// let mut html = txn.get_xml_element("div");
    /// let p = html.push_elem_back(&mut txn, "p");
    /// let txt = p.push_text_back(&mut txn);
    /// txt.push(&mut txn, "Hello ");
    /// let b = p.push_elem_back(&mut txn, "b");
    /// let txt = b.push_text_back(&mut txn);
    /// txt.push(&mut txn, "world");
    /// let txt = html.push_text_back(&mut txn);
    /// txt.push(&mut txn, "again");
    ///
    /// for node in html.successors(&txn) {
    ///     match node {
    ///         Xml::Element(elem) => println!("- {}", elem.tag()),
    ///         Xml::Text(txt) => println!("- {}", txt.to_string(&txn))
    ///     }
    /// }
    /// /* will print:
    ///    - UNDEFINED // (XML root element)
    ///    - p
    ///    - Hello
    ///    - b
    ///    - world
    ///    - again
    /// */
    /// ```
    pub fn successors<'a, 'b, 'txn>(&'a self, txn: &'b Transaction<'txn>) -> TreeWalker<'b, 'txn> {
        self.0.iter(txn)
    }

    /// Inserts another [XmlElement] with a given tag `name` into a current one at the given `index`
    /// and returns it. If `index` is equal to `0`, new element will be inserted as a first child.
    /// If `index` is equal to length of current XML element, new element will be inserted as a last
    /// child.
    /// This method will panic if `index` is greater than the length of current XML element.
    pub fn insert_elem<S: ToString>(
        &self,
        txn: &mut Transaction,
        index: u32,
        name: S,
    ) -> XmlElement {
        self.0.insert_elem(txn, index, name)
    }

    /// Inserts a [XmlText] into a current XML element at the given `index` and returns it.
    /// If `index` is equal to `0`, new text field will be inserted as a first child.
    /// If `index` is equal to length of current XML element, new text field will be inserted
    /// as a last child.
    /// This method will panic if `index` is greater than the length of current XML element.
    pub fn insert_text(&self, txn: &mut Transaction, index: u32) -> XmlText {
        self.0.insert_text(txn, index)
    }

    /// Removes a range (defined by `len`) of XML nodes from the current XML element, starting at
    /// the given `index`. Returns the result which may contain an error if a number of elements
    /// removed is lesser than the expected one provided in `len` parameter.
    pub fn remove_range(&self, txn: &mut Transaction, index: u32, len: u32) {
        self.0.remove(txn, index, len)
    }

    /// Pushes a new [XmlElement] with a given tag `name` as the last child of a current one and
    /// returns it.
    pub fn push_elem_back<S: ToString>(&self, txn: &mut Transaction, name: S) -> XmlElement {
        self.0.push_elem_back(txn, name)
    }

    /// Pushes a new [XmlElement] with a given tag `name` as the first child of a current one and
    /// returns it.
    pub fn push_elem_front<S: ToString>(&self, txn: &mut Transaction, name: S) -> XmlElement {
        self.0.push_elem_front(txn, name)
    }

    /// Pushes a new [XmlText] field as the last child of a current XML element and returns it.
    pub fn push_text_back(&self, txn: &mut Transaction) -> XmlText {
        self.0.push_text_back(txn)
    }

    /// Pushes a new [XmlText] field as the first child of a current XML element and returns it.
    pub fn push_text_front(&self, txn: &mut Transaction) -> XmlText {
        self.0.push_text_front(txn)
    }

    /// Returns an XML node stored under a given `index` of a current XML element.
    /// Returns `None` if provided `index` is over the range of a current element.
    pub fn get(&self, txn: &Transaction, index: u32) -> Option<Xml> {
        self.0.get(txn, index)
    }
}

impl Into<ItemContent> for XmlElement {
    fn into(self) -> ItemContent {
        self.0.into()
    }
}

impl From<BranchRef> for XmlElement {
    fn from(inner: BranchRef) -> Self {
        XmlElement(XmlFragment::new(inner))
    }
}

/// Iterator over the attributes (key-value pairs represented as a strings) of an [XmlElement].
pub struct Attributes<'a, 'txn>(Entries<'a, 'txn>);

impl<'a, 'txn> Iterator for Attributes<'a, 'txn> {
    type Item = (&'a str, String);

    fn next(&mut self) -> Option<Self::Item> {
        let (key, block) = self.0.next()?;
        let value = block
            .content
            .get_content_last(self.0.txn)
            .map(|v| v.to_string(self.0.txn))
            .unwrap_or(String::default());

        Some((key.as_str(), value))
    }
}

impl Into<XmlElement> for XmlFragment {
    fn into(self) -> XmlElement {
        XmlElement(self)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct XmlFragment(BranchRef);

impl XmlFragment {
    pub fn new(inner: BranchRef) -> Self {
        XmlFragment(inner)
    }

    fn inner(&self) -> Ref<Branch> {
        self.0.borrow()
    }

    pub fn first_child(&self, txn: &Transaction) -> Option<Xml> {
        let inner = self.inner();
        let first = inner.first(txn)?;
        match &first.content {
            ItemContent::Type(c) => {
                let value = Xml::from(c.clone());
                Some(value)
            }
            _ => None,
        }
    }

    pub fn parent(&self, txn: &Transaction) -> Option<XmlElement> {
        parent(self.inner(), txn)
    }

    pub fn len(&self, _txn: &Transaction) -> u32 {
        self.inner().len()
    }

    pub fn iter<'a, 'b, 'txn>(&'a self, txn: &'b Transaction<'txn>) -> TreeWalker<'b, 'txn> {
        TreeWalker::new(txn, &*self.inner())
    }

    pub fn to_string(&self, txn: &Transaction) -> String {
        let mut s = String::new();
        let inner = self.inner();
        for i in inner.iter(txn) {
            for content in i.content.get_content(txn) {
                write!(&mut s, "{}", content.to_string(txn)).unwrap();
            }
        }
        s
    }

    pub fn insert_elem<S: ToString>(
        &self,
        txn: &mut Transaction,
        index: u32,
        name: S,
    ) -> XmlElement {
        let item = self
            .0
            .insert_at(txn, index, PrelimXml::Elem(name.to_string()));
        if let ItemContent::Type(inner) = &item.content {
            XmlElement::from(inner.clone())
        } else {
            panic!("Defect: inserted XML element returned primitive value block")
        }
    }

    pub fn insert_text(&self, txn: &mut Transaction, index: u32) -> XmlText {
        let item = self.0.insert_at(txn, index, PrelimXml::Text);
        if let ItemContent::Type(inner) = &item.content {
            XmlText::from(inner.clone())
        } else {
            panic!("Defect: inserted XML element returned primitive value block")
        }
    }

    pub fn remove(&self, txn: &mut Transaction, index: u32, len: u32) {
        let removed = self.0.remove_at(txn, index, len);
        if removed != len {
            panic!("Couldn't remove {} elements from an array. Only {} of them were successfully removed.", len, removed);
        }
    }

    pub fn push_elem_back<S: ToString>(&self, txn: &mut Transaction, name: S) -> XmlElement {
        let len = self.len(txn);
        self.insert_elem(txn, len, name)
    }

    pub fn push_elem_front<S: ToString>(&self, txn: &mut Transaction, name: S) -> XmlElement {
        self.insert_elem(txn, 0, name)
    }

    pub fn push_text_back(&self, txn: &mut Transaction) -> XmlText {
        let len = self.len(txn);
        self.insert_text(txn, len)
    }

    pub fn push_text_front(&self, txn: &mut Transaction) -> XmlText {
        self.insert_text(txn, 0)
    }

    pub fn get<T: From<BranchRef>>(&self, txn: &Transaction, index: u32) -> Option<T> {
        let inner = self.inner();
        let (content, _) = inner.get_at(txn, index)?;
        if let ItemContent::Type(inner) = content {
            Some(T::from(inner.clone()))
        } else {
            None
        }
    }
}

impl Into<ItemContent> for XmlFragment {
    fn into(self) -> ItemContent {
        ItemContent::Type(self.0.clone())
    }
}

/// An iterator over [XmlElement] successors, working in a recursive depth-first manner.
pub struct TreeWalker<'a, 'txn> {
    txn: &'a Transaction<'txn>,
    current: Option<&'a Item>,
    root: TypePtr,
    first_call: bool,
}

impl<'a, 'txn> TreeWalker<'a, 'txn> {
    fn new<'b>(txn: &'a Transaction<'txn>, parent: &'b Branch) -> Self {
        let root = parent.ptr.clone();
        let current = parent
            .start
            .as_ref()
            .and_then(|p| txn.store.blocks.get_item(p));

        TreeWalker {
            txn,
            current,
            root,
            first_call: true,
        }
    }
}

impl<'a, 'txn> Iterator for TreeWalker<'a, 'txn> {
    type Item = Xml;

    /// Tree walker used depth-first search to move over the xml tree.
    fn next(&mut self) -> Option<Self::Item> {
        let mut result = None;
        let mut n = self.current.take();
        if let Some(current) = n {
            if !self.first_call || current.is_deleted() {
                while {
                    if let ItemContent::Type(t) = &current.content {
                        let inner = t.borrow();
                        let type_ref = inner.type_ref();
                        if !current.is_deleted()
                            && (type_ref == TYPE_REFS_XML_ELEMENT || type_ref == TYPE_REFS_XML_FRAGMENT)
                            && inner.start.is_some()
                        {
                            // walk down in the tree
                            n = inner
                                .start
                                .as_ref()
                                .and_then(|ptr| self.txn.store.blocks.get_item(ptr));
                        } else {
                            // walk right or up in the tree
                            while let Some(current) = n {
                                if let Some(right) = current.right.as_ref() {
                                    n = self.txn.store.blocks.get_item(right);
                                    break;
                                } else if current.parent == self.root {
                                    n = None;
                                } else {
                                    n = self
                                        .txn
                                        .store
                                        .get_type(&current.parent)
                                        .and_then(|t| t.as_ref().item.as_ref())
                                        .and_then(|ptr| self.txn.store.blocks.get_item(ptr));
                                }
                            }
                        }
                    }
                    if let Some(current) = n {
                        current.is_deleted()
                    } else {
                        false
                    }
                } {}
            }
            self.first_call = false;
            self.current = n;
        }
        if let Some(current) = self.current {
            if let ItemContent::Type(t) = &current.content {
                result = Some(Xml::from(t.clone()));
            }
        }
        result
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct XmlHook(Map);

impl XmlHook {
    pub fn new(map: Map) -> Self {
        XmlHook(map)
    }

    pub fn len(&self, txn: &Transaction) -> u32 {
        self.0.len(txn)
    }

    pub fn to_json(&self, txn: &Transaction) -> Any {
        self.0.to_json(txn)
    }

    pub fn keys<'a, 'b, 'txn>(
        &'a self,
        txn: &'b Transaction<'txn>,
    ) -> crate::types::map::Keys<'b, 'txn> {
        self.0.keys(txn)
    }

    pub fn values<'a, 'b, 'txn>(
        &self,
        txn: &'b Transaction<'txn>,
    ) -> crate::types::map::Values<'b, 'txn> {
        self.0.values(txn)
    }

    pub fn iter<'a, 'b, 'txn>(
        &self,
        txn: &'b Transaction<'txn>,
    ) -> crate::types::map::MapIter<'b, 'txn> {
        self.0.iter(txn)
    }

    pub fn insert<V: Prelim>(&self, txn: &mut Transaction, key: String, value: V) -> Option<Value> {
        self.0.insert(txn, key, value)
    }

    pub fn remove(&self, txn: &mut Transaction, key: &str) -> Option<Value> {
        self.0.remove(txn, key)
    }

    pub fn get(&self, txn: &Transaction, key: &str) -> Option<Value> {
        self.0.get(txn, key)
    }

    pub fn contains(&self, txn: &Transaction, key: &String) -> bool {
        self.0.contains(txn, key)
    }

    pub fn clear(&self, txn: &mut Transaction) {
        self.0.clear(txn)
    }
}

impl From<BranchRef> for XmlHook {
    fn from(inner: BranchRef) -> Self {
        XmlHook(Map::from(inner))
    }
}

impl Into<XmlHook> for Map {
    fn into(self) -> XmlHook {
        XmlHook(self)
    }
}

/// A shared data type used for collaborative text editing, that can be used in a context of
/// [XmlElement] nodee. It enables multiple users to add and remove chunks of text in efficient
/// manner. This type is internally represented as a mutable double-linked list of text chunks
/// - an optimization occurs during [Transaction::commit], which allows to squash multiple
/// consecutively inserted characters together as a single chunk of text even between transaction
/// boundaries in order to preserve more efficient memory model.
///
/// Just like [XmlElement], [XmlText] can be marked with extra metadata in form of attributes.
///
/// [XmlText] structure internally uses UTF-8 encoding and its length is described in a number of
/// bytes rather than individual characters (a single UTF-8 code point can consist of many bytes).
///
/// Like all Yrs shared data types, [XmlText] is resistant to the problem of interleaving (situation
/// when characters inserted one after another may interleave with other peers concurrent inserts
/// after merging all updates together). In case of Yrs conflict resolution is solved by using
/// unique document id to determine correct and consistent ordering.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct XmlText(Text);

impl XmlText {
    fn inner(&self) -> Ref<Branch> {
        self.0.inner()
    }

    /// Returns a string representation of a current XML text.
    pub fn to_string(&self, txn: &Transaction) -> String {
        self.0.to_string(txn)
    }

    pub fn remove_attribute(&self, txn: &mut Transaction, attr_name: &str) {
        self.inner().remove(txn, attr_name);
    }

    pub fn insert_attribute<K: ToString, V: ToString>(
        &self,
        txn: &mut Transaction,
        attr_name: K,
        attr_value: V,
    ) {
        let key = attr_name.to_string();
        let value = crate::block::PrelimText(attr_value.to_string());
        let pos = {
            let inner = self.inner();
            let left = inner.map.get(&key);
            ItemPosition {
                parent: inner.ptr.clone(),
                left: left.cloned(),
                right: None,
                index: 0,
            }
        };

        txn.create_item(&pos, value, Some(key));
    }

    pub fn get_attribute(&self, txn: &Transaction, attr_name: &str) -> Option<String> {
        let inner: Ref<_> = self.inner();
        let value = inner.get(txn, attr_name)?;
        Some(value.to_string(txn))
    }

    pub fn attributes<'a, 'b, 'txn>(&'a self, txn: &'b Transaction<'txn>) -> Attributes<'b, 'txn> {
        Attributes(self.inner().entries(txn))
    }

    /// Returns next XML sibling of this XML text, which can be either a [XmlElement], [XmlText] or
    /// `None` if current text is a last child of its parent XML element.
    pub fn next_sibling(&self, txn: &Transaction) -> Option<Xml> {
        next_sibling(self.0.inner(), txn)
    }

    /// Returns previous XML sibling of this XML text, which can be either a [XmlElement], [XmlText]
    /// or `None` if current text is a first child of its parent XML element.
    pub fn prev_sibling(&self, txn: &Transaction) -> Option<Xml> {
        prev_sibling(self.0.inner(), txn)
    }

    /// Returns a parent XML element containing this XML text value.
    pub fn parent(&self, txn: &Transaction) -> Option<XmlElement> {
        parent(self.inner(), txn)
    }

    /// Returns a number of characters contained under this XML text structure.
    pub fn len(&self, _txn: &Transaction) -> u32 {
        self.0.len()
    }

    /// Inserts a new string `content` into this XML text structure at the given `index`.
    /// This method may panic if `index` if greater than a length of this text.
    pub fn insert(&self, txn: &mut Transaction, index: u32, content: &str) {
        if let Some(mut pos) = self.0.find_position(txn, index) {
            let parent = { TypePtr::Id(self.inner().item.unwrap()) };
            pos.parent = parent;
            txn.create_item(&pos, crate::block::PrelimText(content.to_owned()), None);
        } else {
            panic!("Cannot insert string content into an XML text: provided index is outside of the current text range!");
        }
    }

    /// Appends a new string `content` at the end of this XML text structure.
    pub fn push(&self, txn: &mut Transaction, content: &str) {
        let len = self.len(txn);
        self.insert(txn, len, content);
    }

    /// Removes a number of characters specified by a `len` parameter from this XML text structure,
    /// starting at given `index`.
    /// This method may panic if `index` if greater than a length of this text.
    pub fn remove_range(&self, txn: &mut Transaction, index: u32, len: u32) {
        self.0.remove_range(txn, index, len)
    }
}

impl Into<ItemContent> for XmlText {
    fn into(self) -> ItemContent {
        self.0.into()
    }
}

impl From<BranchRef> for XmlText {
    fn from(inner: BranchRef) -> Self {
        XmlText(Text::from(inner))
    }
}

impl Into<XmlText> for Text {
    fn into(self) -> XmlText {
        XmlText(self)
    }
}

enum PrelimXml {
    Elem(String),
    Text,
}

impl Prelim for PrelimXml {
    fn into_content(self, txn: &mut Transaction, ptr: TypePtr) -> (ItemContent, Option<Self>) {
        let inner = match self {
            PrelimXml::Elem(node_name) => {
                BranchRef::new(Branch::new(ptr, TYPE_REFS_XML_ELEMENT, Some(node_name)))
            }
            PrelimXml::Text => BranchRef::new(Branch::new(ptr, TYPE_REFS_XML_TEXT, None)),
        };
        (ItemContent::Type(inner), None)
    }

    fn integrate(self, txn: &mut Transaction, inner_ref: BranchRef) {}
}

fn next_sibling(inner: Ref<Branch>, txn: &Transaction) -> Option<Xml> {
    let mut current = inner
        .item
        .as_ref()
        .and_then(|ptr| txn.store.blocks.get_item(ptr));
    while let Some(item) = current {
        current = item
            .right
            .as_ref()
            .and_then(|ptr| txn.store.blocks.get_item(ptr));
        if let Some(right) = current {
            if !right.is_deleted() {
                if let ItemContent::Type(inner) = &right.content {
                    return Some(Xml::from(inner.clone()));
                }
            }
        }
    }

    None
}

fn prev_sibling(inner: Ref<Branch>, txn: &Transaction) -> Option<Xml> {
    let mut current = inner
        .item
        .as_ref()
        .and_then(|ptr| txn.store.blocks.get_item(ptr));
    while let Some(item) = current {
        current = item
            .left
            .as_ref()
            .and_then(|ptr| txn.store.blocks.get_item(ptr));
        if let Some(left) = current {
            if !left.is_deleted() {
                if let ItemContent::Type(inner) = &left.content {
                    return Some(Xml::from(inner.clone()));
                }
            }
        }
    }

    None
}

fn parent(inner: Ref<Branch>, txn: &Transaction) -> Option<XmlElement> {
    let item = txn.store.blocks.get_item(inner.item.as_ref()?)?;
    let parent = txn.store.get_type(&item.parent)?;
    Some(XmlElement::from(parent.clone()))
}

#[cfg(test)]
mod test {
    use crate::types::xml::Xml;
    use crate::Doc;

    #[test]
    fn insert_attribute() {
        let d1 = Doc::with_client_id(1);
        let mut t1 = d1.transact();
        let xml1 = t1.get_xml_element("xml");
        xml1.insert_attribute(&mut t1, "height", 10);
        assert_eq!(xml1.get_attribute(&t1, "height"), Some("10".to_string()));

        let d2 = Doc::with_client_id(1);
        let mut t2 = d2.transact();
        let xml2 = t2.get_xml_element("xml");
        d2.apply_update_v1(&mut t2, d1.encode_state_as_update_v1(&t1).as_slice());
        assert_eq!(xml2.get_attribute(&t2, "height"), Some("10".to_string()));
    }

    #[test]
    fn tree_walker() {
        let doc = Doc::with_client_id(1);
        let mut txn = doc.transact();
        /*
            <UNDEFINED>
                <p>{txt1}{txt2}</p>
                <p></p>
                <img/>
            </UNDEFINED>
        */
        let root = txn.get_xml_element("xml");
        let p1 = root.push_elem_back(&mut txn, "p");
        p1.push_text_back(&mut txn);
        p1.push_text_back(&mut txn);
        let p2 = root.push_elem_back(&mut txn, "p");
        root.push_elem_back(&mut txn, "img");

        let all_paragraphs = root.successors(&txn).filter_map(|n| match n {
            Xml::Element(e) if e.tag() == "p" => Some(e),
            _ => None,
        });
        let actual: Vec<_> = all_paragraphs.collect();

        assert_eq!(
            actual.len(),
            2,
            "query selector should found two paragraphs"
        );
        assert_eq!(actual[0], p1, "query selector found 1st paragraph");
        assert_eq!(actual[1], p2, "query selector found 2nd paragraph");
    }

    #[test]
    fn text_attributes() {
        let doc = Doc::with_client_id(1);
        let mut txn = doc.transact();
        let txt = txn.get_xml_text("txt");
        txt.insert_attribute(&mut txn, "test", 42);

        assert_eq!(txt.get_attribute(&txn, "test"), Some("42".to_string()));
        let actual: Vec<_> = txt.attributes(&txn).collect();
        assert_eq!(actual, vec![("test", "42".to_string())]);
    }

    #[test]
    fn siblings() {
        let doc = Doc::with_client_id(1);
        let mut txn = doc.transact();
        let root = txn.get_xml_element("root");
        let first = root.push_text_back(&mut txn);
        first.push(&mut txn, "hello");
        let second = root.push_elem_back(&mut txn, "p");

        assert_eq!(
            first.next_sibling(&txn).as_ref(),
            Some(&Xml::Element(second.clone())),
            "first.next_sibling should point to second"
        );
        assert_eq!(
            second.prev_sibling(&txn).as_ref(),
            Some(&Xml::Text(first.clone())),
            "second.prev_sibling should point to first"
        );
        assert_eq!(
            first.parent(&txn).as_ref(),
            Some(&root),
            "first.parent should point to root"
        );
        assert_eq!(
            root.parent(&txn).as_ref(),
            None,
            "root parent should not exist"
        );
        assert_eq!(
            root.first_child(&txn).as_ref(),
            Some(&Xml::Text(first)),
            "root.first_child should point to first"
        );
    }

    #[test]
    fn serialization() {
        let d1 = Doc::with_client_id(1);
        let mut t1 = d1.transact();
        let r1 = t1.get_xml_element("root");
        let first = r1.push_text_back(&mut t1);
        first.push(&mut t1, "hello");
        let second = r1.push_elem_back(&mut t1, "p");

        let expected = "<UNDEFINED>hello<p></p></UNDEFINED>";
        assert_eq!(r1.to_string(&t1), expected);

        let u1 = d1.encode_state_as_update_v1(&t1);

        let d2 = Doc::with_client_id(2);
        let mut t2 = d2.transact();
        let r2 = t2.get_xml_element("root");

        d2.apply_update_v1(&mut t2, u1.as_slice());
        assert_eq!(r2.to_string(&t2), expected);
    }

    #[test]
    fn serialization_compatibility() {
        let d1 = Doc::with_client_id(1);
        let mut t1 = d1.transact();
        let r1 = t1.get_xml_element("root");
        let first = r1.push_text_back(&mut t1);
        first.push(&mut t1, "hello");
        let second = r1.push_elem_back(&mut t1, "p");

        /* This binary is result of following Yjs code (matching Rust code above):
        ```js
            let d1 = new Y.Doc()
            d1.clientID = 1
            let root = d1.get('root', Y.XmlElement)
            let first = new Y.XmlText()
            first.insert(0, 'hello')
            let second = new Y.XmlElement('p')
            root.insert(0, [first,second])

            let expected = Y.encodeStateAsUpdate(d1)
        ``` */
        let expected = &[
            1, 3, 1, 0, 7, 1, 4, 114, 111, 111, 116, 6, 4, 0, 1, 0, 5, 104, 101, 108, 108, 111,
            135, 1, 0, 3, 1, 112, 0,
        ];
        let u1 = d1.encode_state_as_update_v1(&t1);
        assert_eq!(u1.as_slice(), expected);
    }
}
