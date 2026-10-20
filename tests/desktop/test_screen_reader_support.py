"""Observer configuration cannot select audio modules or interpolate commands."""
import tempfile,unittest,socket
from pathlib import Path
from unittest.mock import patch
from screen_reader_speech import configure,utterances,transport_messages,plain_text,retire
class Observer(unittest.TestCase):
 def test_fixed_true_argument_and_generic_only_directory(self):
  with tempfile.TemporaryDirectory() as directory:
   root=Path(directory);binary=root/'bin';binary.mkdir();true=binary/'true';true.write_text('#!/bin/sh\nexit 0\n');true.chmod(0o700)
   target=root/'observer';target.mkdir()
   with patch('screen_reader_speech.execution_shell_proof',return_value={}):state=configure(target,root/'dispatcher',true)
   self.assertEqual(sorted(value.name for value in (target/'speech-modules').iterdir()),['sd_generic'])
   config=(target/'speech-config/modules/observer.conf').read_text();self.assertIn('GenericExecuteSynth "'+str(true)+" '$DATA'\"",config);self.assertNotIn('$PLAY_COMMAND',config)
   self.assertEqual(state['address'],'unix_socket:'+str(target/'speech.sock'))
   self.assertIn('--module-dir='+str(target/'speech-modules'),state['command'])
 def test_retirement_keeps_logs_and_link_target(self):
  with tempfile.TemporaryDirectory() as directory:
   root=Path(directory);target=root/'keep';target.write_text('retained');(root/'speech-modules').mkdir();(root/'speech-modules/sd_generic').symlink_to(target)
   with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as endpoint:endpoint.bind(str(root/'speech.sock'))
   retire(root)
   self.assertFalse((root/'speech.sock').exists());self.assertFalse((root/'speech-modules/sd_generic').is_symlink());self.assertEqual(target.read_text(),'retained')
 def test_other_commands_rejected(self):
  with tempfile.TemporaryDirectory() as directory:
   with self.assertRaises(ValueError):configure(directory,directory,'/bin/false')
 def test_frames_require_queue_ack_and_keep_lifecycle(self):
  protocol=" 24:DATA:|SET self CLIENT_NAME unknown:Orca:default\n| (1)\n 24:DATA:|SPEAK\n| (1)\n 24:DATA:|Sentinel Alpha\n| (1)\n 24:DATA:|.\n| (1)\n 24:REPLY:|225-7\n225 OK MESSAGE QUEUED\n|\n 24:REPLY:|701-7\n701-1\n701 BEGIN\n|\n 24:REPLY:|702-7\n702-1\n702 END\n|\n"
  result=transport_messages(protocol);self.assertEqual(len(result),1);self.assertEqual(result[0]['text'],'Sentinel Alpha');self.assertEqual(result[0]['client'],'unknown:Orca:default');self.assertEqual(result[0]['events'],['BEGIN','END'])
  self.assertEqual(transport_messages(protocol[:protocol.index(' 24:REPLY:')]),[])
 def test_ssml_marks_preserve_words(self):
  self.assertEqual(plain_text('<speak>Sentinel <mark name="1"/>Alpha</speak>'),"Sentinel Alpha")
 def test_real_debug_lines_keep_order(self):
  self.assertEqual(utterances("x SPEECH OUTPUT: 'Alpha' voice\nx SPEECH OUTPUT: 'Beta' voice"),["'Alpha' voice","'Beta' voice"])
if __name__=='__main__':unittest.main()
