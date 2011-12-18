#!/usr/bin/python

import sys
import re

serial_done = False

host = 'simir'
ttl = '1H'
typ = 'AAAA'
addrs = ('2002:564:546::1', '2002:5546:81b4:1:21f:29ff:fe9b:29e4', '2002:5547:81b4:1:21f:29ff:fe9b:29e4')
done = False

zone_fn = sys.argv[1]
out_fn = sys.argv[2]

zone_file = open(zone_fn, 'r')
out_file = open(out_fn, 'w')

for line in zone_file:
	if 'erial' in line:
		if not serial_done:
			#serial = re.match('.*[0-9]+.*', line)
			serial = re.search('(\d+)', line).group(0)
			serial = int(serial)
			line = line.replace(str(serial), str(serial+1))

			out_file.write(line+'\n')

			serial_done = True
		#endif

		continue
	#endif

	if line.startswith(host):
		m = re.match('(\S+)\t(\S+)\t(\S+)\t(\S+)', line)
		if not m:
			print 'record for \'%s\' in wrong format' % line
			continue
		#endif

		m_host, m_ttl, m_typ, m_addr = m.groups()
		print m
		print m.groups()

		if not done:
			for a in addrs:
				out_file.write('%s\t%s\t%s\t%s\n' % (host, ttl, typ, a))
			#endfor

			done = True
		#endif

		continue
	#endif

	out_file.write(line)
#endfor

zone_file.close()
out_file.close()

