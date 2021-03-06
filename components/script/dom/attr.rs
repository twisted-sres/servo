/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::dom::bindings::cell::{DomRefCell, Ref};
use crate::dom::bindings::codegen::Bindings::AttrBinding::AttrMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::root::{DomRoot, LayoutDom, MutNullableDom};
use crate::dom::bindings::str::DOMString;
use crate::dom::customelementregistry::CallbackReaction;
use crate::dom::document::Document;
use crate::dom::element::{AttributeMutation, Element};
use crate::dom::mutationobserver::{Mutation, MutationObserver};
use crate::dom::node::Node;
use crate::dom::virtualmethods::vtable_for;
use crate::script_thread::ScriptThread;
use devtools_traits::AttrInfo;
use dom_struct::dom_struct;
use html5ever::{LocalName, Namespace, Prefix};
use servo_atoms::Atom;
use std::borrow::ToOwned;
use std::mem;
use style::attr::{AttrIdentifier, AttrValue};

// https://dom.spec.whatwg.org/#interface-attr
#[dom_struct]
pub struct Attr {
    node_: Node,
    identifier: AttrIdentifier,
    value: DomRefCell<AttrValue>,

    /// the element that owns this attribute.
    owner: MutNullableDom<Element>,
}

impl Attr {
    fn new_inherited(
        document: &Document,
        local_name: LocalName,
        value: AttrValue,
        name: LocalName,
        namespace: Namespace,
        prefix: Option<Prefix>,
        owner: Option<&Element>,
    ) -> Attr {
        Attr {
            node_: Node::new_inherited(document),
            identifier: AttrIdentifier {
                local_name: local_name,
                name: name,
                namespace: namespace,
                prefix: prefix,
            },
            value: DomRefCell::new(value),
            owner: MutNullableDom::new(owner),
        }
    }

    pub fn new(
        document: &Document,
        local_name: LocalName,
        value: AttrValue,
        name: LocalName,
        namespace: Namespace,
        prefix: Option<Prefix>,
        owner: Option<&Element>,
    ) -> DomRoot<Attr> {
        Node::reflect_node(
            Box::new(Attr::new_inherited(
                document, local_name, value, name, namespace, prefix, owner,
            )),
            document,
        )
    }

    #[inline]
    pub fn name(&self) -> &LocalName {
        &self.identifier.name
    }

    #[inline]
    pub fn namespace(&self) -> &Namespace {
        &self.identifier.namespace
    }

    #[inline]
    pub fn prefix(&self) -> Option<&Prefix> {
        self.identifier.prefix.as_ref()
    }
}

impl AttrMethods for Attr {
    // https://dom.spec.whatwg.org/#dom-attr-localname
    fn LocalName(&self) -> DOMString {
        // FIXME(ajeffrey): convert directly from LocalName to DOMString
        DOMString::from(&**self.local_name())
    }

    // https://dom.spec.whatwg.org/#dom-attr-value
    fn Value(&self) -> DOMString {
        // FIXME(ajeffrey): convert directly from AttrValue to DOMString
        DOMString::from(&**self.value())
    }

    // https://dom.spec.whatwg.org/#dom-attr-value
    fn SetValue(&self, value: DOMString) {
        if let Some(owner) = self.owner() {
            let value = owner.parse_attribute(&self.identifier.namespace, self.local_name(), value);
            self.set_value(value, &owner);
        } else {
            *self.value.borrow_mut() = AttrValue::String(value.into());
        }
    }

    // https://dom.spec.whatwg.org/#dom-attr-name
    fn Name(&self) -> DOMString {
        // FIXME(ajeffrey): convert directly from LocalName to DOMString
        DOMString::from(&*self.identifier.name)
    }

    // https://dom.spec.whatwg.org/#dom-attr-namespaceuri
    fn GetNamespaceURI(&self) -> Option<DOMString> {
        match self.identifier.namespace {
            ns!() => None,
            ref url => Some(DOMString::from(&**url)),
        }
    }

    // https://dom.spec.whatwg.org/#dom-attr-prefix
    fn GetPrefix(&self) -> Option<DOMString> {
        // FIXME(ajeffrey): convert directly from LocalName to DOMString
        self.prefix().map(|p| DOMString::from(&**p))
    }

    // https://dom.spec.whatwg.org/#dom-attr-ownerelement
    fn GetOwnerElement(&self) -> Option<DomRoot<Element>> {
        self.owner()
    }

    // https://dom.spec.whatwg.org/#dom-attr-specified
    fn Specified(&self) -> bool {
        true // Always returns true
    }
}

impl Attr {
    pub fn set_value(&self, mut value: AttrValue, owner: &Element) {
        let name = self.local_name().clone();
        let namespace = self.namespace().clone();
        let old_value = DOMString::from(&**self.value());
        let new_value = DOMString::from(&*value);
        let mutation = Mutation::Attribute {
            name: name.clone(),
            namespace: namespace.clone(),
            old_value: Some(old_value.clone()),
        };

        MutationObserver::queue_a_mutation_record(owner.upcast::<Node>(), mutation);

        if owner.get_custom_element_definition().is_some() {
            let reaction = CallbackReaction::AttributeChanged(
                name,
                Some(old_value),
                Some(new_value),
                namespace,
            );
            ScriptThread::enqueue_callback_reaction(owner, reaction, None);
        }

        assert_eq!(Some(owner), self.owner().as_deref());
        owner.will_mutate_attr(self);
        self.swap_value(&mut value);
        if self.identifier.namespace == ns!() {
            vtable_for(owner.upcast())
                .attribute_mutated(self, AttributeMutation::Set(Some(&value)));
        }
    }

    /// Used to swap the attribute's value without triggering mutation events
    pub fn swap_value(&self, value: &mut AttrValue) {
        mem::swap(&mut *self.value.borrow_mut(), value);
    }

    pub fn identifier(&self) -> &AttrIdentifier {
        &self.identifier
    }

    pub fn value(&self) -> Ref<AttrValue> {
        self.value.borrow()
    }

    pub fn local_name(&self) -> &LocalName {
        &self.identifier.local_name
    }

    /// Sets the owner element. Should be called after the attribute is added
    /// or removed from its older parent.
    pub fn set_owner(&self, owner: Option<&Element>) {
        let ns = &self.identifier.namespace;
        match (self.owner(), owner) {
            (Some(old), None) => {
                // Already gone from the list of attributes of old owner.
                assert!(
                    old.get_attribute(&ns, &self.identifier.local_name)
                        .as_deref() !=
                        Some(self)
                )
            },
            (Some(old), Some(new)) => assert_eq!(&*old, new),
            _ => {},
        }
        self.owner.set(owner);
    }

    pub fn owner(&self) -> Option<DomRoot<Element>> {
        self.owner.get()
    }

    pub fn summarize(&self) -> AttrInfo {
        AttrInfo {
            namespace: (*self.identifier.namespace).to_owned(),
            name: String::from(self.Name()),
            value: String::from(self.Value()),
        }
    }

    pub fn qualified_name(&self) -> DOMString {
        match self.prefix() {
            Some(ref prefix) => DOMString::from(format!("{}:{}", prefix, &**self.local_name())),
            None => DOMString::from(&**self.local_name()),
        }
    }
}

#[allow(unsafe_code)]
pub trait AttrHelpersForLayout {
    unsafe fn value_forever(&self) -> &'static AttrValue;
    unsafe fn value_ref_forever(&self) -> &'static str;
    unsafe fn value_tokens_forever(&self) -> Option<&'static [Atom]>;
    unsafe fn local_name_atom_forever(&self) -> LocalName;
}

#[allow(unsafe_code)]
impl AttrHelpersForLayout for LayoutDom<Attr> {
    #[inline]
    unsafe fn value_forever(&self) -> &'static AttrValue {
        // This transmute is used to cheat the lifetime restriction.
        mem::transmute::<&AttrValue, &AttrValue>((*self.unsafe_get()).value.borrow_for_layout())
    }

    #[inline]
    unsafe fn value_ref_forever(&self) -> &'static str {
        &**self.value_forever()
    }

    #[inline]
    unsafe fn value_tokens_forever(&self) -> Option<&'static [Atom]> {
        // This transmute is used to cheat the lifetime restriction.
        match *self.value_forever() {
            AttrValue::TokenList(_, ref tokens) => Some(tokens),
            _ => None,
        }
    }

    #[inline]
    unsafe fn local_name_atom_forever(&self) -> LocalName {
        (*self.unsafe_get()).identifier.local_name.clone()
    }
}
