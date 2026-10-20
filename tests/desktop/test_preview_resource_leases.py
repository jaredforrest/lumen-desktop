"""Real private resource bounds, shared leases and in-flight retirement."""
import os
from pathlib import Path
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
from preview_snapshots import PreviewSnapshots,PreviewRequests

class ResourceLeases(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory();self.root=Path(self.temporary.name)
        self.environment=patch.dict(os.environ,{'XDG_RUNTIME_DIR':str(self.root)});self.environment.start()
        self.resources=PreviewSnapshots(self.identity,self.validate)
    def tearDown(self):
        self.resources.close();self.environment.stop();self.temporary.cleanup()
    def identity(self,path,info=None):
        info=info or Path(path).stat(follow_symlinks=False)
        return repr((str(path),info.st_dev,info.st_ino,info.st_size,info.st_mtime_ns,info.st_ctime_ns))
    def validate(self,path,token):
        if self.identity(path)!=token:raise ValueError('Changed fixture')
    def source(self,name,size=32):
        path=self.root/name
        with path.open('wb') as stream:stream.truncate(size)
        return path,self.identity(path)
    def test_provider_lifetime_baseline_exhausts_but_retired_working_set_does_not(self):
        files=[self.source(str(i)+'.bmp',40*1024*1024) for i in range(4)]
        for path,token in files[:3]:self.resources.capture(path,token)
        with self.assertRaisesRegex(ValueError,'working set'):self.resources.capture(*files[3])
        self.resources.release(self.resources.provider_lease)
        uris=[]
        for path,token in files*2:
            lease=self.resources.begin()
            with self.resources.pin(lease):resource=self.resources.capture(path,token,lease)
            self.assertEqual(self.resources.preview_bytes,40*1024*1024)
            self.assertTrue(resource.exists());uris.append(str(resource))
            self.resources.release(lease);self.assertFalse(resource.exists())
        self.assertEqual(len(set(uris)),len(uris));self.assertEqual(self.resources.preview_bytes,0)
    def test_shared_resource_survives_until_last_exact_lease_retires(self):
        path,token=self.source('shared');first=self.resources.begin();second=self.resources.begin()
        resource=self.resources.capture(path,token,first)
        self.assertEqual(self.resources.capture(path,token,second),resource)
        self.assertEqual(self.resources.preview_bytes,32)
        self.resources.release(first);self.assertTrue(resource.exists())
        self.resources.release('unknown-or-another-provider');self.assertTrue(resource.exists())
        self.resources.release(second);self.assertFalse(resource.exists())
    def test_retired_before_dispatch_never_creates_resource(self):
        path,token=self.source('retired');lease=self.resources.begin();self.resources.release(lease)
        with self.assertRaises(ValueError):
            with self.resources.pin(lease):self.resources.capture(path,token,lease)
        self.assertEqual(self.resources.preview_bytes,0)
    def test_worker_pin_prevents_retirement_or_teardown_while_in_use(self):
        path,token=self.source('busy');lease=self.resources.begin();captured=threading.Event();finish=threading.Event();facts={}
        def work():
            with self.resources.pin(lease):
                facts['resource']=self.resources.capture(path,token,lease);captured.set()
                if not finish.wait(3):raise AssertionError('Fixture pin deadline')
        worker=threading.Thread(target=work);worker.start()
        try:
            self.assertTrue(captured.wait(3));self.assertFalse(self.resources.release(lease))
            self.assertTrue(facts['resource'].exists());self.assertFalse(self.resources.wait_released(lease,.01))
            with self.assertRaisesRegex(ValueError,'workers'):self.resources.close()
        finally:finish.set();worker.join(3)
        self.assertFalse(worker.is_alive());self.assertTrue(self.resources.wait_released(lease,.1));self.assertFalse(facts['resource'].exists())
    def test_helper_inputs_drop_without_touching_other_live_owner(self):
        source,token=self.source('source');rendered,rendered_token=self.source('page');lease=self.resources.begin()
        original=self.resources.capture(source,token,lease);raster=self.resources.capture(rendered,rendered_token,lease)
        other=PreviewSnapshots(self.identity,self.validate)
        try:
            other_resource=other.capture(source,token)
            self.resources.keep(lease,[raster]);self.assertFalse(original.exists());self.assertTrue(raster.exists())
            self.resources.release(lease);self.assertFalse(raster.exists());self.assertTrue(other_resource.exists())
        finally:other.close()
    def test_page_source_transfers_but_previous_raster_retires(self):
        requests=PreviewRequests(self.resources);source,token=self.source('document.pdf');first,first_token=self.source('first.png');second,second_token=self.source('second.png')
        requests.register(1,token)
        with requests.work(1) as lease:
            retained_source=self.resources.capture(source,token,lease);requests.retain_source(1,token)
            raster=self.resources.capture(first,first_token,lease);requests.finish(1,[raster])
        requests.retire([1],keep_source_token=token)
        self.assertTrue(retained_source.exists());self.assertFalse(raster.exists())
        requests.register(2,token)
        with requests.work(2) as lease:
            self.assertEqual(self.resources.capture(source,token,lease),retained_source)
            requests.retain_source(2,token);next_raster=self.resources.capture(second,second_token,lease);requests.finish(2,[next_raster])
        requests.retire([2]);self.assertFalse(retained_source.exists());self.assertFalse(next_raster.exists());self.assertEqual(self.resources.preview_bytes,0)
        requests.retire([1,2,999])
        with self.assertRaises(ValueError):requests.register(1,token)
    def test_retirement_precedes_dispatch_and_cannot_resurrect_source(self):
        requests=PreviewRequests(self.resources);source,token=self.source('queued');requests.register(1,token);requests.retire([1])
        with self.assertRaises(ValueError):
            with requests.work(1) as lease:self.resources.capture(source,token,lease)
        self.assertFalse(requests.sources);self.assertEqual(self.resources.preview_bytes,0)
    def test_cancel_inflight_request_keeps_helper_input_until_pin_unwinds(self):
        requests=PreviewRequests(self.resources);source,token=self.source('inflight.pdf');requests.register(1,token)
        captured=threading.Event();finish=threading.Event();facts={};errors=[]
        def work():
            try:
                with requests.work(1) as lease:
                    resource=self.resources.capture(source,token,lease);requests.retain_source(1,token)
                    facts['resource']=resource;captured.set()
                    if not finish.wait(3):raise AssertionError('Fixture worker deadline')
                    self.assertEqual(resource.read_bytes(),bytes(32))
                    with self.assertRaises(ValueError):requests.finish(1,[resource])
                requests.retire([1])
            except BaseException as error:errors.append(error)
        worker=threading.Thread(target=work);worker.start()
        try:
            self.assertTrue(captured.wait(3));requests.retire([1],wait=False)
            self.assertTrue(facts['resource'].exists());self.assertFalse(requests.sources)
        finally:finish.set();worker.join(3)
        self.assertFalse(worker.is_alive());self.assertEqual(errors,[]);self.assertFalse(facts['resource'].exists());self.assertEqual(self.resources.preview_bytes,0)

if __name__=='__main__':unittest.main()
