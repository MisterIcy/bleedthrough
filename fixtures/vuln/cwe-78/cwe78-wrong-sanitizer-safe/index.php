<?php

$value = escapeshellarg($_GET['x']);
system('ls -l ' . $value);
