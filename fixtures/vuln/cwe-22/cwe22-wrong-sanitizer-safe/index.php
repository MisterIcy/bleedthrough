<?php

$value = basename($_GET['x']);
readfile('/var/www/files/' . $value);
