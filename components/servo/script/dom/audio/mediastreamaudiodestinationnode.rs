/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use js::rust::HandleObject;
use crate::media::ServoMedia;
use crate::media::audio::node::AudioNodeInit;
use crate::media::streams::MediaStreamType;

use crate::script::dom::audio::audiocontext::AudioContext;
use crate::script::dom::audio::audionode::{AudioNode, AudioNodeOptionsHelper};
use crate::script::dom::bindings::codegen::Bindings::AudioNodeBinding::{
    AudioNodeOptions, ChannelCountMode, ChannelInterpretation,
};
use crate::script::dom::bindings::codegen::GenericBindings::MediaStreamAudioDestinationNodeBinding::MediaStreamAudioDestinationNodeMethods;
use crate::script::dom::bindings::error::Fallible;
use crate::script::dom::bindings::inheritance::Castable;
use crate::script::dom::bindings::reflector::{DomGlobal, reflect_dom_object_with_proto};
use crate::script::dom::bindings::root::{Dom, DomRoot};
use crate::script::dom::mediastream::MediaStream;
use crate::script::dom::window::Window;
use crate::script::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct MediaStreamAudioDestinationNode {
    node: AudioNode,
    stream: Dom<MediaStream>,
}

impl MediaStreamAudioDestinationNode {
    #[cfg_attr(crown, expect(crown::unrooted_must_root))]
    pub(crate) fn new_inherited(
        context: &AudioContext,
        options: &AudioNodeOptions,
        can_gc: CanGc,
    ) -> Fallible<MediaStreamAudioDestinationNode> {
        let media = ServoMedia::get();
        let (socket, id) = media.create_stream_and_socket(MediaStreamType::Audio);
        let stream = MediaStream::new_single(&context.global(), id, MediaStreamType::Audio, can_gc);
        let node_options = options.unwrap_or(
            2,
            ChannelCountMode::Explicit,
            ChannelInterpretation::Speakers,
        );
        let node = AudioNode::new_inherited(
            AudioNodeInit::MediaStreamDestinationNode(socket),
            context.upcast(),
            node_options,
            1, // inputs
            0, // outputs
        )?;
        Ok(MediaStreamAudioDestinationNode {
            node,
            stream: Dom::from_ref(&stream),
        })
    }

    pub(crate) fn new(
        window: &Window,
        context: &AudioContext,
        options: &AudioNodeOptions,
        can_gc: CanGc,
    ) -> Fallible<DomRoot<MediaStreamAudioDestinationNode>> {
        Self::new_with_proto(window, None, context, options, can_gc)
    }

    #[cfg_attr(crown, expect(crown::unrooted_must_root))]
    fn new_with_proto(
        window: &Window,
        proto: Option<HandleObject>,
        context: &AudioContext,
        options: &AudioNodeOptions,
        can_gc: CanGc,
    ) -> Fallible<DomRoot<MediaStreamAudioDestinationNode>> {
        let node = MediaStreamAudioDestinationNode::new_inherited(context, options, can_gc)?;
        Ok(reflect_dom_object_with_proto(
            Box::new(node),
            window,
            proto,
            can_gc,
        ))
    }
}

impl MediaStreamAudioDestinationNodeMethods<crate::DomTypeHolder>
    for MediaStreamAudioDestinationNode
{
    /// <https://webaudio.github.io/web-audio-api/#dom-mediastreamaudiodestinationnode-mediastreamaudiodestinationnode>
    fn Constructor(
        window: &Window,
        proto: Option<HandleObject>,
        can_gc: CanGc,
        context: &AudioContext,
        options: &AudioNodeOptions,
    ) -> Fallible<DomRoot<MediaStreamAudioDestinationNode>> {
        MediaStreamAudioDestinationNode::new_with_proto(window, proto, context, options, can_gc)
    }

    /// <https://webaudio.github.io/web-audio-api/#dom-mediastreamaudiodestinationnode-stream>
    fn Stream(&self) -> DomRoot<MediaStream> {
        DomRoot::from_ref(&self.stream)
    }
}
